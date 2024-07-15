use crate::circuits::cell::*;
use crate::circuits::etable::allocator::*;
use crate::circuits::etable::ConstraintBuilder;
use crate::circuits::etable::EventTableCommonArgsConfig;
use crate::circuits::etable::EventTableCommonConfig;
use crate::circuits::etable::EventTableOpcodeConfig;
use crate::circuits::etable::EventTableOpcodeConfigBuilder;
use crate::circuits::utils::bn_to_field;
use crate::circuits::utils::step_status::StepStatus;
use crate::circuits::utils::table_entry::EventTableEntryWithMemoryInfo;
use crate::circuits::utils::Context;
use crate::constant;
use crate::constant_from;
use halo2_proofs::arithmetic::FieldExt;
use halo2_proofs::plonk::Error;
use halo2_proofs::plonk::Expression;
use halo2_proofs::plonk::VirtualCells;
use num_bigint::BigUint;
use specs::encode::opcode::encode_conversion;
use specs::encode::opcode::UniArgEncode;
use specs::etable::EventTableEntry;
use specs::mtable::LocationType;
use specs::mtable::VarType;
use specs::step::StepInfo;

pub struct ConversionConfig<F: FieldExt> {
    value_arg: EventTableCommonArgsConfig<F>,

    value: AllocatedU64Cell<F>,
    value_is_i8: AllocatedBitCell<F>,
    value_is_i16: AllocatedBitCell<F>,
    value_is_i32: AllocatedBitCell<F>,
    value_is_i64: AllocatedBitCell<F>,
    res_is_i32: AllocatedBitCell<F>,
    res_is_i64: AllocatedBitCell<F>,

    sign_op: AllocatedBitCell<F>,
    is_i32_wrap_i64: AllocatedBitCell<F>,

    // Sign-extension proposal
    flag_bit: AllocatedBitCell<F>,

    rem: AllocatedU64Cell<F>,
    rem_helper: AllocatedU64Cell<F>,
    d: AllocatedU64Cell<F>,
    modulus: AllocatedUnlimitedCell<F>,
    shift: AllocatedUnlimitedCell<F>,
    padding: AllocatedUnlimitedCell<F>,

    memory_table_lookup_stack_write: AllocatedMemoryTableLookupWriteCell<F>,
}

pub struct ConversionConfigBuilder {}

impl<F: FieldExt> EventTableOpcodeConfigBuilder<F> for ConversionConfigBuilder {
    fn configure(
        common_config: &EventTableCommonConfig<F>,
        allocator: &mut EventTableCellAllocator<F>,
        constraint_builder: &mut ConstraintBuilder<F>,
    ) -> Box<dyn EventTableOpcodeConfig<F>> {
        let value = allocator.alloc_u64_cell();

        let value_is_i8 = allocator.alloc_bit_cell();
        let value_is_i16 = allocator.alloc_bit_cell();
        let value_is_i32 = allocator.alloc_bit_cell();
        let value_is_i64 = allocator.alloc_bit_cell();

        let res_is_i32 = allocator.alloc_bit_cell();
        let res_is_i64 = allocator.alloc_bit_cell();

        let sign_op = allocator.alloc_bit_cell();
        let is_i32_wrap_i64 = allocator.alloc_bit_cell();

        let flag_bit = allocator.alloc_bit_cell();
        let shift = allocator.alloc_unlimited_cell();
        let padding = allocator.alloc_unlimited_cell();

        let d = allocator.alloc_u64_cell();
        let rem = allocator.alloc_u64_cell();
        let rem_helper = allocator.alloc_u64_cell();
        let modulus = allocator.alloc_unlimited_cell();

        let eid = common_config.eid_cell;
        let sp = common_config.sp_cell;

        let value_arg = common_config.uniarg_configs[0].clone();
        let value_type_is_i32 = value_arg.is_i32_cell;
        constraint_builder.push(
            "select: uniarg",
            Box::new(move |meta| vec![value_arg.value_cell.expr(meta) - value.expr(meta)]),
        );

        let uniarg_configs = common_config.uniarg_configs.clone();
        let memory_table_lookup_stack_write = allocator
            .alloc_memory_table_lookup_write_cell_with_value(
                "op_conversion stack write",
                constraint_builder,
                eid,
                move |____| constant_from!(LocationType::Stack as u64),
                move |meta| Self::sp_after_uniarg(sp, &uniarg_configs, meta),
                move |meta| res_is_i32.expr(meta),
                move |____| constant_from!(1),
            );

        let res = memory_table_lookup_stack_write.value_cell;

        /*
         * Implicit Constraint:
         *
         * value_is_i8 || value_is_i16 || value_is_i32 || value_is_i64 can be constrained by opcode.
         * res_is_i32  || res_is_i64 can be constrained by opcode.
         */

        constraint_builder.push(
            "op_conversion i32_wrap_i64",
            Box::new(move |meta| {
                vec![
                    is_i32_wrap_i64.expr(meta)
                        * (value_is_i64.expr(meta) + res_is_i32.expr(meta) - constant_from!(2)),
                    is_i32_wrap_i64.expr(meta)
                        * (value.u16_cells_le[1].expr(meta) * constant_from!(1 << 16)
                            + value.u16_cells_le[0].expr(meta)
                            - res.expr(meta)),
                ]
            }),
        );

        constraint_builder.push(
            "op_conversion helper",
            Box::new(move |meta| {
                vec![
                    // In order to make i32.wrap_i64 satisfies the "op_conversion: sign extension"
                    // constraint, setting the shift value to `1<<31` when value_is_i64.
                    shift.expr(meta)
                        - (value_is_i8.expr(meta) * constant_from!(1u64 << 7)
                            + value_is_i16.expr(meta) * constant_from!(1u64 << 15)
                            + (value_is_i32.expr(meta) + value_is_i64.expr(meta))
                                * constant_from!(1u64 << 31)),
                    padding.expr(meta)
                        - (value_is_i8.expr(meta) * constant_from!((u32::MAX << 8) as u64)
                            + value_is_i16.expr(meta) * constant_from!((u32::MAX << 16) as u64)
                            + res_is_i64.expr(meta) * constant_from!(u64::MAX << 32)),
                    modulus.expr(meta) - shift.expr(meta) * constant_from!(2),
                ]
            }),
        );

        constraint_builder.push(
            "op_conversion: split operand",
            Box::new(move |meta| {
                vec![
                    /*
                     * split value into (out of range part, sign flag, rem)
                     * e.g. supports i32.extend_i8s but operand is 0x100
                     */
                    value.expr(meta)
                        - d.expr(meta) * modulus.expr(meta)
                        - flag_bit.expr(meta) * shift.expr(meta)
                        - rem.expr(meta),
                    // rem must less than shift
                    rem.expr(meta) + constant_from!(1) + rem_helper.expr(meta) - shift.expr(meta),
                ]
            }),
        );

        constraint_builder.push(
            "op_conversion: sign extension",
            Box::new(move |meta| {
                vec![
                    // Compose Result for all extend instructions
                    flag_bit.expr(meta) * padding.expr(meta) * sign_op.expr(meta)
                        + flag_bit.expr(meta) * shift.expr(meta)
                        + rem.expr(meta)
                        - res.expr(meta),
                ]
            }),
        );

        Box::new(ConversionConfig {
            value_arg,
            value,
            value_is_i8,
            value_is_i16,
            value_is_i32,
            value_is_i64,
            res_is_i32,
            res_is_i64,
            sign_op,
            is_i32_wrap_i64,
            flag_bit,
            d,
            rem,
            rem_helper,
            modulus,
            shift,
            padding,
            memory_table_lookup_stack_write,
        })
    }
}

impl<F: FieldExt> EventTableOpcodeConfig<F> for ConversionConfig<F> {
    fn opcode(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        encode_conversion::<Expression<F>>(
            self.sign_op.expr(meta),
            self.value_arg.is_i32_cell.expr(meta),
            self.value_is_i8.expr(meta),
            self.value_is_i16.expr(meta),
            self.value_is_i32.expr(meta),
            self.value_is_i64.expr(meta),
            self.res_is_i32.expr(meta),
            self.res_is_i64.expr(meta),
            UniArgEncode::Reserve,
        )
    }

    fn assign(
        &self,
        ctx: &mut Context<'_, F>,
        step: &mut StepStatus<F>,
        entry: &EventTableEntryWithMemoryInfo,
    ) -> Result<(), Error> {
        let (is_sign_op, value, value_type, result, result_type, padding, shift) =
            match &entry.eentry.step_info {
                StepInfo::I32WrapI64 { value, result } => {
                    self.value_is_i64.assign_bool(ctx, true)?;
                    self.res_is_i32.assign_bool(ctx, true)?;
                    self.is_i32_wrap_i64.assign_bool(ctx, true)?;

                    (
                        false,
                        *value as u64,
                        VarType::I64,
                        *result as u32 as u64,
                        VarType::I32,
                        0,
                        1u64 << 31, // To meet `op_conversion: sign extension` constraint
                    )
                }
                StepInfo::I64ExtendI32 {
                    value,
                    result,
                    sign,
                } => {
                    self.value_is_i32.assign_bool(ctx, true)?;
                    self.res_is_i64.assign_bool(ctx, true)?;

                    (
                        *sign,
                        *value as u32 as u64,
                        VarType::I32,
                        *result as u64,
                        VarType::I64,
                        u64::MAX << 32,
                        1 << 31,
                    )
                }
                StepInfo::I32SignExtendI8 { value, result } => {
                    self.value_is_i8.assign_bool(ctx, true)?;
                    self.res_is_i32.assign_bool(ctx, true)?;

                    (
                        true,
                        *value as u32 as u64,
                        VarType::I32,
                        *result as u32 as u64,
                        VarType::I32,
                        (u32::MAX << 8) as u64,
                        1 << 7,
                    )
                }
                StepInfo::I32SignExtendI16 { value, result } => {
                    self.value_is_i16.assign_bool(ctx, true)?;
                    self.res_is_i32.assign_bool(ctx, true)?;

                    (
                        true,
                        *value as u32 as u64,
                        VarType::I32,
                        *result as u32 as u64,
                        VarType::I32,
                        (u32::MAX << 16) as u64,
                        1 << 15,
                    )
                }
                StepInfo::I64SignExtendI8 { value, result } => {
                    self.value_is_i8.assign_bool(ctx, true)?;
                    self.res_is_i64.assign_bool(ctx, true)?;

                    (
                        true,
                        *value as u64,
                        VarType::I64,
                        *result as u64,
                        VarType::I64,
                        u64::MAX << 8,
                        1 << 7,
                    )
                }
                StepInfo::I64SignExtendI16 { value, result } => {
                    self.value_is_i16.assign_bool(ctx, true)?;
                    self.res_is_i64.assign_bool(ctx, true)?;

                    (
                        true,
                        *value as u64,
                        VarType::I64,
                        *result as u64,
                        VarType::I64,
                        u64::MAX << 16,
                        1 << 15,
                    )
                }
                StepInfo::I64SignExtendI32 { value, result } => {
                    self.value_is_i32.assign_bool(ctx, true)?;
                    self.res_is_i64.assign_bool(ctx, true)?;

                    (
                        true,
                        *value as u64,
                        VarType::I64,
                        *result as u64,
                        VarType::I64,
                        u64::MAX << 32,
                        1 << 31,
                    )
                }
                _ => unreachable!(),
            };

        self.value.assign(ctx, value)?;
        self.res_is_i32.assign(ctx, F::from(result_type as u64))?;
        self.sign_op.assign_bool(ctx, is_sign_op)?;

        let modulus = shift << 1;
        let rem = (value % modulus) & (shift - 1);
        let flag_bit = ((value & shift) != 0) as u64;

        self.d.assign(ctx, value / modulus)?;
        self.rem.assign(ctx, rem)?;
        self.rem_helper.assign(ctx, shift - 1 - rem)?;
        self.flag_bit.assign(ctx, flag_bit.into())?;
        self.shift.assign(ctx, F::from(shift))?;
        self.modulus
            .assign(ctx, bn_to_field(&BigUint::from(modulus)))?;
        self.padding.assign(ctx, F::from(padding))?;

        if let specs::itable::Opcode::Conversion { uniarg, .. } =
            entry.eentry.get_instruction(&step.current.itable).opcode
        {
            let mut memory_entries = entry.memory_rw_entires.iter();

            self.value_arg.assign(ctx, uniarg, &mut memory_entries)?;
        } else {
            unreachable!();
        }

        self.memory_table_lookup_stack_write.assign(
            ctx,
            step.current.eid,
            entry.memory_rw_entires[1].end_eid,
            step.current.sp + 1,
            LocationType::Stack,
            result_type == VarType::I32,
            result,
        )?;

        Ok(())
    }

    fn mops(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant_from!(1))
    }

    fn memory_writing_ops(&self, _: &EventTableEntry) -> u32 {
        1
    }

    fn sp_diff(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant!(-F::one()))
    }
}
