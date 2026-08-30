mod call_abi;
mod call_frames;
mod concat;
mod indirect_access;
mod int_map;
mod int_sort;
mod numeric_lowering;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[
    call_abi::SPEC,
    call_frames::SPEC,
    concat::SPEC,
    indirect_access::SPEC,
    int_map::SPEC,
    int_sort::SPEC,
    numeric_lowering::SPEC,
    transformer::SPEC,
];
