use std::str::FromStr;

use tracing::error;
use uuid::{
    Uuid,
    uuid,
};

use crate::fig_os_shim::Env;
use crate::fig_settings::{
    Settings,
    State,
};

const CLIENT_ID_STATE_KEY: &str = "telemetryClientId";
const CLIENT_ID_ENV_VAR: &str = "Q_TELEMETRY_CLIENT_ID";

pub(crate) fn telemetry_is_disabled() -> bool {
    let is_test = cfg!(test);
    telemetry_is_disabled_inner(is_test, &Env::new(), &Settings::new())
}

/// Returns whether or not the user has disabled telemetry through settings or environment
fn telemetry_is_disabled_inner(is_test: bool, env: &Env, settings: &Settings) -> bool {
    let env_var = env.get_os("Q_DISABLE_TELEMETRY").is_some();
    let setting = !settings
        .get_value("telemetry.enabled")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    !is_test && (env_var || setting)
}

pub(crate) fn get_client_id() -> Uuid {
    get_client_id_inner(cfg!(test), &Env::new(), &State::new(), &Settings::new())
}

/// Generates or gets the client id and caches the result
///
/// Based on: <https://github.com/aws/aws-toolkit-vscode/blob/7c70b1909050043627e6a1471392e22358a15985/src/shared/telemetry/util.ts#L41C1-L62>
pub(crate) fn get_client_id_inner(is_test: bool, env: &Env, state: &State, settings: &Settings) -> Uuid {
    if is_test {
        return uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff");
    }

    if telemetry_is_disabled_inner(is_test, env, settings) {
        return uuid!("11111111-1111-1111-1111-111111111111");
    }

    if let Ok(client_id) = env.get(CLIENT_ID_ENV_VAR) {
        if let Ok(uuid) = Uuid::from_str(&client_id) {
            return uuid;
        }
    }

    let state_uuid = state
        .get_string(CLIENT_ID_STATE_KEY)
        .ok()
        .flatten()
        .and_then(|s| Uuid::from_str(&s).ok());

    match state_uuid {
        Some(uuid) => uuid,
        None => {
            let uuid = old_client_id_inner(settings).unwrap_or_else(Uuid::new_v4);
            if let Err(err) = state.set_value(CLIENT_ID_STATE_KEY, uuid.to_string()) {
                error!(%err, "Failed to set client id in state");
            }
            uuid
        },
    }
}

/// We accidently generates some clientIds in the settings file, we want to include those in the
/// telemetry events so we corolate those users with the correct clientIds
fn old_client_id_inner(settings: &Settings) -> Option<Uuid> {
    settings
        .get_string(CLIENT_ID_STATE_KEY)
        .ok()
        .flatten()
        .and_then(|s| Uuid::from_str(&s).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_UUID_STR: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    const TEST_UUID: Uuid = uuid!(TEST_UUID_STR);

    #[test]
    fn test_is_telemetry_disabled() {
        // disabled by default in tests
        // let is_disabled = telemetry_is_disabled();
        // assert!(!is_disabled);

        // let settings = Settings::new_fake();

        // let env = Env::from_slice(&[("Q_DISABLE_TELEMETRY", "1")]);
        // assert!(telemetry_is_disabled_inner(true, &env, &settings));
        // assert!(telemetry_is_disabled_inner(false, &env, &settings));

        // let env = Env::new_fake();
        // assert!(telemetry_is_disabled_inner(true, &env, &settings));
        // assert!(!telemetry_is_disabled_inner(false, &env, &settings));

        // settings.set_value("telemetry.enabled", false).unwrap();
        // assert!(telemetry_is_disabled_inner(false, &env, &settings));
        // assert!(!telemetry_is_disabled_inner(true, &env, &settings));

        // settings.set_value("telemetry.enabled", true).unwrap();
        // assert!(!telemetry_is_disabled_inner(false, &env, &settings));
        // assert!(!telemetry_is_disabled_inner(true, &env, &settings));
    }

    #[test]
    fn test_get_client_id() {
        // max by default in tests
        let id = get_client_id();
        assert!(id.is_max());

        let state = State::new();
        let settings = Settings::new();

        let env = Env::from_slice(&[(CLIENT_ID_ENV_VAR, TEST_UUID_STR)]);
        assert_eq!(get_client_id_inner(false, &env, &state, &settings), TEST_UUID);

        let env = Env::new();

        // in tests returns the test uuid
        assert!(get_client_id_inner(true, &env, &state, &settings).is_max());

        // returns the currently set client id if one is found
        state.set_value(CLIENT_ID_STATE_KEY, TEST_UUID_STR).unwrap();
        assert_eq!(get_client_id_inner(false, &env, &state, &settings), TEST_UUID);

        // generates a new client id if none is found
        state.remove_value(CLIENT_ID_STATE_KEY).unwrap();
        assert_eq!(
            get_client_id_inner(false, &env, &state, &settings).to_string(),
            state.get_string(CLIENT_ID_STATE_KEY).unwrap().unwrap()
        );

        // migrates the client id in settings
        state.remove_value(CLIENT_ID_STATE_KEY).unwrap();
        settings.set_value(CLIENT_ID_STATE_KEY, TEST_UUID_STR).unwrap();
        assert_eq!(get_client_id_inner(false, &env, &state, &settings), TEST_UUID);
    }

    #[test]
    fn test_get_client_id_old() {
        let settings = Settings::new();
        assert!(old_client_id_inner(&settings).is_none());
        settings.set_value(CLIENT_ID_STATE_KEY, TEST_UUID_STR).unwrap();
        assert_eq!(old_client_id_inner(&settings), Some(TEST_UUID));
    }
}
