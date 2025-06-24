use std::mem::MaybeUninit;

use accessibility_sys::{
    AXError,
    kAXErrorSuccess,
};

#[allow(clippy::missing_safety_doc)]
pub unsafe fn ax_call<F, V>(f: F) -> Result<V, AXError>
where
    F: Fn(*mut V) -> AXError,
{
    let mut result = MaybeUninit::uninit();
    let err = (f)(result.as_mut_ptr());

    if err != kAXErrorSuccess {
        return Err(err);
    }

    Ok(result.assume_init())
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn bool_ax_call<F, V>(f: F) -> Result<V, bool>
where
    F: Fn(*mut V) -> bool,
{
    let mut result = MaybeUninit::uninit();
    let err = (f)(result.as_mut_ptr());

    if !err {
        return Err(err);
    }

    Ok(result.assume_init())
}

pub(crate) unsafe fn ax_call_void<F>(f: F) -> Result<(), AXError>
where
    F: Fn() -> AXError,
{
    let err = (f)();

    if err != kAXErrorSuccess {
        return Err(err);
    }

    Ok(())
}
