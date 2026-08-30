mod call_abi;
mod concat;
mod indirect_access;
mod int_map;
mod int_sort;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[
    call_abi::SPEC,
    concat::SPEC,
    indirect_access::SPEC,
    int_map::SPEC,
    int_sort::SPEC,
    transformer::SPEC,
];
