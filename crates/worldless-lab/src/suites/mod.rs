mod concat;

use crate::SuiteSpec;

pub(crate) const REGISTRY: &[SuiteSpec] = &[concat::SPEC];
