use crate::DisplayError;

pub(crate) fn require_contexts(process_pmv2: bool, thread_pmv2: bool) -> Result<(), DisplayError> {
    if process_pmv2 && thread_pmv2 {
        Ok(())
    } else {
        Err(DisplayError::DpiContextMismatch {
            process_pmv2,
            thread_pmv2,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_and_thread_must_both_be_pmv2_without_override_fallback() {
        assert_eq!(require_contexts(true, true), Ok(()));
        for (process_pmv2, thread_pmv2) in [(false, false), (false, true), (true, false)] {
            assert_eq!(
                require_contexts(process_pmv2, thread_pmv2).unwrap_err(),
                DisplayError::DpiContextMismatch {
                    process_pmv2,
                    thread_pmv2
                }
            );
        }
    }
}
