mod aggregate_layout;
mod call_abi;
mod call_frames;
mod concat;
mod dynamic_vector;
mod indirect_access;
mod int_map;
mod int_sort;
mod loop_lowering;
mod numeric_lowering;
mod register_pressure;
mod result_abi;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[
    aggregate_layout::SPEC,
    call_abi::SPEC,
    call_frames::SPEC,
    concat::SPEC,
    dynamic_vector::SPEC,
    indirect_access::SPEC,
    int_map::SPEC,
    int_sort::SPEC,
    loop_lowering::SPEC,
    numeric_lowering::SPEC,
    register_pressure::SPEC,
    result_abi::SPEC,
    transformer::SPEC,
];
