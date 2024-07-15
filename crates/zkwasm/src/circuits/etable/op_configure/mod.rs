pub mod op_bin;
pub mod op_bin_bit;
pub mod op_bin_shift;
pub mod op_br;
pub mod op_br_if;
pub mod op_br_if_eqz;
pub mod op_br_table;
pub mod op_call;
pub mod op_call_host_foreign_circuit;
pub mod op_call_indirect;
pub mod op_const;
pub mod op_conversion;
pub mod op_drop;
pub mod op_global_get;
pub mod op_global_set;
pub mod op_load;
pub mod op_local_get;
pub mod op_local_set;
pub mod op_local_tee;
pub mod op_memory_grow;
pub mod op_memory_size;
pub mod op_rel;
pub mod op_return;
pub mod op_select;
pub mod op_store;
pub mod op_test;
pub mod op_unary;

pub(crate) struct UniArgDesc<T> {
    pub(crate) is_stack: T,
    pub(crate) is_pop: T,
    pub(crate) is_const: T,
    pub(crate) value: T,
}
