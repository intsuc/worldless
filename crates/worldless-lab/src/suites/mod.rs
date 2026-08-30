mod concat;
mod int_sort;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[concat::SPEC, int_sort::SPEC, transformer::SPEC];
