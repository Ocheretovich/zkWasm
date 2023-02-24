use crate::{
    circuits::{
        cell::*,
        etable_v2::{
            allocator::*, ConstraintBuilder, EventTableCommonConfig, EventTableOpcodeConfig,
            EventTableOpcodeConfigBuilder,
        },
        jtable::{expression::JtableLookupEntryEncode, JumpTableConfig},
        utils::{
            bn_to_field, step_status::StepStatus, table_entry::EventTableEntryWithMemoryInfo,
            Context,
        },
    },
    constant, constant_from,
};
use halo2_proofs::{
    arithmetic::FieldExt,
    plonk::{Error, Expression, VirtualCells},
};
use num_bigint::BigUint;
use specs::{
    encode::{frame_table::encode_frame_table_entry, opcode::encode_call},
    etable::EventTableEntry,
    itable::{OpcodeClass, OPCODE_ARG0_SHIFT, OPCODE_CLASS_SHIFT},
    mtable::{LocationType, VarType},
    step::StepInfo,
};

pub struct CallConfig<F: FieldExt> {
    index_cell: AllocatedCommonRangeCell<F>,
    frame_table_lookup: AllocatedUnlimitedCell<F>,
}

pub struct CallConfigBuilder {}

impl<F: FieldExt> EventTableOpcodeConfigBuilder<F> for CallConfigBuilder {
    fn configure(
        common_config: &EventTableCommonConfig<F>,
        allocator: &mut EventTableCellAllocator<F>,
        constraint_builder: &mut ConstraintBuilder<F>,
    ) -> Box<dyn EventTableOpcodeConfig<F>> {
        let index_cell = allocator.alloc_common_range_cell();
        let frame_table_lookup = common_config.jtable_lookup_cell;

        let fid_cell = common_config.fid_cell;
        let iid_cell = common_config.iid_cell;
        let frame_id_cell = common_config.frame_id_cell;
        let eid = common_config.eid_cell;

        constraint_builder.constraints.push((
            "return frame table lookups",
            Box::new(move |meta| {
                vec![
                    frame_table_lookup.expr(meta)
                        - JumpTableConfig::encode_lookup(
                            eid.expr(meta),
                            frame_id_cell.expr(meta),
                            index_cell.expr(meta),
                            fid_cell.expr(meta),
                            iid_cell.expr(meta) + constant_from!(1),
                        ),
                ]
            }),
        ));

        Box::new(CallConfig {
            index_cell,
            frame_table_lookup,
        })
    }
}

impl<F: FieldExt> EventTableOpcodeConfig<F> for CallConfig<F> {
    fn opcode(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        encode_call(self.index_cell.expr(meta))
    }

    fn assign(
        &self,
        ctx: &mut Context<'_, F>,
        step: &StepStatus,
        entry: &EventTableEntryWithMemoryInfo,
    ) -> Result<(), Error> {
        match &entry.eentry.step_info {
            StepInfo::Call { index } => {
                self.index_cell.assign(ctx, F::from(*index as u64))?;
                self.frame_table_lookup.assign(
                    ctx,
                    bn_to_field(&encode_frame_table_entry(
                        step.current.eid.into(),
                        step.current.last_jump_eid.into(),
                        (*index).into(),
                        step.current.fid.into(),
                        (step.current.iid + 1).into(),
                    )),
                )?;
                Ok(())
            }

            _ => unreachable!(),
        }
    }

    fn jops_expr(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant_from!(1))
    }

    fn jops(&self) -> u32 {
        1
    }

    fn next_frame_id(
        &self,
        meta: &mut VirtualCells<'_, F>,
        common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        Some(common_config.eid_cell.curr_expr(meta))
    }

    fn next_fid(
        &self,
        meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        Some(self.index_cell.expr(meta))
    }

    fn next_iid(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        Some(constant_from!(0))
    }
}
