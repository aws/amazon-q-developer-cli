pub fn spawn_auth_watcher() {
    tokio::spawn(async {
        loop {
            if fig_settings::state::get_bool_or("desktop.completedOnboarding", false) && !fig_auth::is_logged_in().await
            {
                let _ = fig_settings::state::set_value("desktop.auth-watcher.logged-in", false);
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    });
}
