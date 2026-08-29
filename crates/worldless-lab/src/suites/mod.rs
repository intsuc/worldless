mod concat;
mod transformer;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[concat::SPEC, transformer::SPEC];
