mod aggregate_layout;
mod call_abi;
mod call_frames;
mod concat;
mod dynamic_vector;
mod i64_lowering;
mod indirect_access;
mod int_map;
mod int_sort;
mod loop_lowering;
mod numeric_lowering;
mod predicate_lowering;
mod register_pressure;
mod result_abi;
mod scalar_replacement;
mod tagged_union_layout;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[
    aggregate_layout::SPEC,
    call_abi::SPEC,
    call_frames::SPEC,
    concat::SPEC,
    dynamic_vector::SPEC,
    i64_lowering::SPEC,
    indirect_access::SPEC,
    int_map::SPEC,
    int_sort::SPEC,
    loop_lowering::SPEC,
    numeric_lowering::SPEC,
    predicate_lowering::SPEC,
    register_pressure::SPEC,
    result_abi::SPEC,
    scalar_replacement::SPEC,
    tagged_union_layout::SPEC,
    transformer::SPEC,
];
