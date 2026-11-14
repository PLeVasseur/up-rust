//! Diagnostic assertions for test fixture construction; never part of the SDK.

pub(crate) trait FixtureResult {
    type Value;
    #[track_caller]
    fn require_fixture(self) -> Self::Value;
}

impl<T, E: core::fmt::Debug> FixtureResult for Result<T, E> {
    type Value = T;
    #[track_caller]
    #[expect(clippy::panic, reason = "A failed fixture must fail its test, with the actual error and caller location")]
    fn require_fixture(self) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("test fixture failed: {error:?}"),
        }
    }
}

impl<T> FixtureResult for Option<T> {
    type Value = T;
    #[track_caller]
    #[expect(clippy::panic, reason = "Missing fixture data must fail its test at the caller location")]
    fn require_fixture(self) -> T {
        match self {
            Some(value) => value,
            None => panic!("test fixture value was absent"),
        }
    }
}
