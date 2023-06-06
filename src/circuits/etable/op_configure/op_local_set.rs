use crate::circuits::cell::*;
use crate::circuits::etable::allocator::*;
use crate::circuits::etable::stack_lookup_context::StackReadLookup;
use crate::circuits::etable::ConstraintBuilder;
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
use specs::etable::EventTableEntry;
use specs::itable::OpcodeClass;
use specs::itable::OPCODE_ARG0_SHIFT;
use specs::itable::OPCODE_CLASS_SHIFT;
use specs::mtable::LocationType;
use specs::mtable::VarType;
use specs::step::StepInfo;

pub struct LocalSetConfig<F: FieldExt> {
    offset_cell: AllocatedCommonRangeCell<F>,
    is_i32_cell: AllocatedBitCell<F>,
    stack_reading_lookup: StackReadLookup<F>,
    memory_table_lookup_stack_write: AllocatedMemoryTableLookupWriteCell<F>,
}

pub struct LocalSetConfigBuilder {}

impl<F: FieldExt> EventTableOpcodeConfigBuilder<F> for LocalSetConfigBuilder {
    fn configure(
        common_config: &EventTableCommonConfig<F>,
        allocator: &mut EventTableCellAllocator<F>,
        constraint_builder: &mut ConstraintBuilder<F>,
    ) -> Box<dyn EventTableOpcodeConfig<F>> {
        let mut stack_lookup_context = common_config.stack_lookup_context.clone();

        let stack_reading_lookup = stack_lookup_context.pop(constraint_builder).unwrap();

        let is_i32_cell = stack_reading_lookup.is_i32;
        let offset_cell = allocator.alloc_common_range_cell();

        let sp_cell = common_config.sp_cell;
        let eid_cell = common_config.eid_cell;

        let memory_table_lookup_stack_write = allocator.alloc_memory_table_lookup_write_cell(
            "op_local_set stack write",
            constraint_builder,
            eid_cell,
            move |____| constant_from!(LocationType::Stack as u64),
            move |meta| sp_cell.expr(meta) + constant_from!(1) + offset_cell.expr(meta),
            move |meta| is_i32_cell.expr(meta),
            move |meta| stack_reading_lookup.value.u64_cell.expr(meta),
            move |____| constant_from!(1),
        );

        Box::new(LocalSetConfig {
            offset_cell,
            is_i32_cell,
            stack_reading_lookup,
            memory_table_lookup_stack_write,
        })
    }
}

impl<F: FieldExt> EventTableOpcodeConfig<F> for LocalSetConfig<F> {
    fn opcode(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        constant!(bn_to_field(
            &(BigUint::from(OpcodeClass::LocalSet as u64) << OPCODE_CLASS_SHIFT)
        )) + self.is_i32_cell.expr(meta)
            * constant!(bn_to_field(&(BigUint::from(1u64) << OPCODE_ARG0_SHIFT)))
            + self.offset_cell.expr(meta)
    }

    fn assign(
        &self,
        ctx: &mut Context<'_, F>,
        step: &StepStatus,
        entry: &EventTableEntryWithMemoryInfo,
    ) -> Result<(), Error> {
        match &entry.eentry.step_info {
            StepInfo::SetLocal {
                vtype,
                depth,
                value,
            } => {
                self.offset_cell.assign(ctx, F::from(*depth as u64))?;

                self.stack_reading_lookup.assign(
                    ctx,
                    entry.memory_rw_entires[0].start_eid,
                    step.current.eid,
                    entry.memory_rw_entires[0].end_eid,
                    step.current.sp + 1,
                    *vtype == VarType::I32,
                    *value,
                )?;

                self.memory_table_lookup_stack_write.assign(
                    ctx,
                    step.current.eid,
                    entry.memory_rw_entires[1].end_eid,
                    step.current.sp + 1 + depth,
                    LocationType::Stack,
                    *vtype == VarType::I32,
                    *value,
                )?;

                Ok(())
            }

            _ => unreachable!(),
        }
    }

    fn sp_diff(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant!(F::one()))
    }

    fn mops(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant_from!(1))
    }

    fn memory_writing_ops(&self, _: &EventTableEntry) -> u32 {
        1
    }
}
