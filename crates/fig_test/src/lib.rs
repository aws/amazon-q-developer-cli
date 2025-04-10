pub use fig_test_macro::{
    test,
    test_async,
};
use tokio::sync::Mutex;

pub static ENVIRONMENT_LOCK: Mutex<Option<Vec<(String, String)>>> = Mutex::const_new(None);

#[cfg(test)]
mod tests {
    use crate as fig_test;

    #[fig_test::test]
    fn test_env() {
        std::env::set_var("BAR", "1");
        assert_eq!(std::env::var("BAR").unwrap(), "1");
    }

    #[fig_test::test_async]
    async fn test_env_async() {
        std::env::set_var("BAR", "1");
        assert_eq!(std::env::var("BAR").unwrap(), "1");
    }

    #[fig_test::test]
    fn stress_test_1() {
        for _ in 0..10000 {
            std::env::set_var("Q_TEST_VAR", "1");
            assert_eq!(std::env::var("Q_TEST_VAR").unwrap(), "1");
            std::env::set_var("Q_TEST_VAR", "2");
        }
    }

    #[fig_test::test]
    fn stress_test_2() {
        for _ in 0..10000 {
            std::env::set_var("Q_TEST_VAR", "3");
            assert_eq!(std::env::var("Q_TEST_VAR").unwrap(), "3");
            std::env::set_var("Q_TEST_VAR", "4");
        }
    }
}
