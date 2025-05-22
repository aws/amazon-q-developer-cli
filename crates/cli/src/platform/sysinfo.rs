use std::ffi::OsString;
use std::sync::{
    Arc,
    Mutex,
};

#[derive(Debug, Clone, Default)]
pub struct SysInfo(inner::Inner);

mod inner {
    use std::collections::HashSet;
    use std::sync::{
        Arc,
        Mutex,
    };

    #[derive(Debug, Clone, Default)]
    pub enum Inner {
        #[default]
        Real,
        Fake(Arc<Mutex<Fake>>),
    }

    #[derive(Debug, Clone, Default)]
    pub struct Fake {
        pub process_names: HashSet<String>,
    }
}

impl SysInfo {
    pub fn new() -> Self {
        match cfg!(test) {
            true => Self(inner::Inner::Fake(Arc::new(Mutex::new(inner::Fake::default())))),
            false => Self(inner::Inner::Real),
        }
    }

    /// Returns whether the process containing `name` is running.
    pub fn is_process_running(&self, name: &str) -> bool {
        use inner::Inner;
        match &self.0 {
            Inner::Real => {
                let system = sysinfo::System::new_all();
                let is_running = system.processes_by_name(&OsString::from(name)).next().is_some();
                is_running
            },
            Inner::Fake(fake) => fake.lock().unwrap().process_names.contains(name),
        }
    }

    pub fn add_running_processes(&self, process_names: &[&str]) {
        use inner::Inner;
        match &self.0 {
            Inner::Real => panic!("unimplemented"),
            Inner::Fake(fake) => {
                let curr_names = &mut fake.lock().unwrap().process_names;
                for name in process_names {
                    curr_names.insert((*name).to_string());
                }
            },
        }
    }
}
