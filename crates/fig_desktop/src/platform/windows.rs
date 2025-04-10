use std::borrow::Cow;
use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::sync::{
    Arc,
    LazyLock,
};

use anyhow::anyhow;
use parking_lot::Mutex;
use tao::dpi::Position;
use tracing::{
    debug,
    trace,
};
use windows::Win32::Foundation::{
    HWND,
    RECT,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER,
    CoCreateInstance,
    CoInitialize,
    VARIANT,
    VARIANT_0,
    VARIANT_0_0,
    VARIANT_0_0_0,
    VT_BOOL,
};
use windows::Win32::System::ProcessStatus::K32GetProcessImageFileNameA;
use windows::Win32::System::Threading::{
    OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    AccessibleObjectFromEvent,
    CUIAutomation,
    HWINEVENTHOOK,
    IUIAutomation,
    IUIAutomationFocusChangedEventHandler,
    IUIAutomationFocusChangedEventHandler_Impl,
    IUIAutomationTextPattern,
    SetWinEventHook,
    TextUnit_Character,
    TreeScope_Descendants,
    UIA_HasKeyboardFocusPropertyId,
    UIA_IsTextPatternAvailablePropertyId,
    UIA_TextPatternId,
    UnhookWinEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF,
    EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND,
    GetForegroundWindow,
    GetWindowThreadProcessId,
    OBJECT_IDENTIFIER,
    OBJID_CARET,
    OBJID_WINDOW,
    WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};
use windows::core::implement;

use crate::event::{
    Event,
    RelativeDirection,
    WindowEvent,
};
use crate::platform::{
    PlatformBoundEvent,
    PlatformWindow,
};
use crate::protocol::icons::{
    AssetSpecifier,
    ProcessedAsset,
};
use crate::utils::Rect;
use crate::webview::{
    FigWindowMap,
    WindowId,
};
use crate::{
    AUTOCOMPLETE_ID,
    EventLoopProxy,
    EventLoopWindowTarget,
};

const VT_TRUE: VARIANT = VARIANT {
    Anonymous: VARIANT_0 {
        Anonymous: ManuallyDrop::new(VARIANT_0_0 {
            vt: VT_BOOL,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: VARIANT_0_0_0 {
                boolVal: unsafe { std::mem::transmute(0xffff_u16) },
            },
        }),
    },
};

static UNMANAGED: LazyLock<Mutex<Unmanaged>> = LazyLock::new(|| {
    Mutex::new(Unmanaged {
        event_sender: None,
        location_hook: None,
        console_state: ConsoleState::None,
    })
});

#[derive(Debug)]
pub struct PlatformStateImpl {
    proxy: EventLoopProxy,
    automation: Automation,
    _focus_changed_event_handler: AutomationEventHandler,
}

impl PlatformStateImpl {
    pub fn new(proxy: EventLoopProxy) -> Self {
        unsafe {
            CoInitialize(None).unwrap();
            let automation = Automation(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap());

            let focus_changed_event_handler: IUIAutomationFocusChangedEventHandler = FocusChangedEventHandler {
                event_loop_proxy: proxy.clone(),
            }
            .into();

            automation
                .0
                .AddFocusChangedEventHandler(None, &focus_changed_event_handler)
                .unwrap();

            Self {
                proxy,
                automation,
                _focus_changed_event_handler: AutomationEventHandler(focus_changed_event_handler),
            }
        }
    }

    pub fn handle(
        self: &Arc<Self>,
        event: PlatformBoundEvent,
        _: &EventLoopWindowTarget,
        _: &FigWindowMap,
    ) -> anyhow::Result<()> {
        match event {
            PlatformBoundEvent::Initialize => {
                UNMANAGED.lock().event_sender.replace(self.proxy.clone());

                unsafe {
                    update_focused_state(GetForegroundWindow());

                    SetWinEventHook(
                        EVENT_SYSTEM_FOREGROUND,
                        EVENT_SYSTEM_FOREGROUND,
                        None,
                        Some(win_event_proc),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    );
                }

                Ok(())
            },
            PlatformBoundEvent::InitializePostRun => {
                trace!("Ignoring InitializePostRun event");
                Ok(())
            },
            PlatformBoundEvent::EditBufferChanged => unsafe {
                let console_state = UNMANAGED.lock().console_state;
                match console_state {
                    ConsoleState::None => Ok(()),
                    ConsoleState::Console { hwnd } => {
                        let automation = &self.automation.0;
                        let window = automation.ElementFromHandle(hwnd)?;

                        let interest = automation.CreateAndCondition(
                            &automation.CreatePropertyCondition(
                                UIA_HasKeyboardFocusPropertyId.0.try_into().unwrap(),
                                &VT_TRUE,
                            )?,
                            &automation.CreatePropertyCondition(
                                UIA_IsTextPatternAvailablePropertyId.0.try_into().unwrap(),
                                &VT_TRUE,
                            )?,
                        )?;

                        let inner = window.FindFirst(TreeScope_Descendants, &interest)?;
                        let text_pattern = inner
                            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId.0.try_into().unwrap())?;
                        let selection = text_pattern.GetSelection()?;
                        let caret = selection.GetElement(0)?;
                        caret.ExpandToEnclosingUnit(TextUnit_Character)?;

                        let bounds = caret.GetBoundingRectangles()?;
                        let mut elements = std::ptr::null_mut::<RECT>();
                        let mut elements_len = 0;

                        automation.SafeArrayToRectNativeArray(&*bounds, &mut elements, &mut elements_len)?;

                        if elements_len > 0 {
                            let bounds = *elements;
                            let height = bounds.top - bounds.bottom;

                            self.proxy.send_event(Event::WindowEvent {
                                window_id: AUTOCOMPLETE_ID,
                                window_event: WindowEvent::PositionRelativeToRect {
                                    x: bounds.left,
                                    y: bounds.bottom - height,
                                    width: bounds.right - bounds.left,
                                    height,
                                    direction: RelativeDirection::Below,
                                },
                            })?;

                            Ok(())
                        } else {
                            Err(anyhow!("Failed to acquire caret position"))
                        }
                    },
                    ConsoleState::Accessible { x, y, width, height } => {
                        self.proxy.send_event(Event::WindowEvent {
                            window_id: AUTOCOMPLETE_ID,
                            window_event: WindowEvent::PositionRelativeToRect {
                                x,
                                y,
                                width,
                                height,
                                direction: RelativeDirection::Below,
                            },
                        })?;

                        Ok(())
                    },
                }
            },
            PlatformBoundEvent::FullscreenStateUpdated { .. } => {
                trace!("Ignoring full screen state updated event");
                Ok(())
            },
            PlatformBoundEvent::AccessibilityUpdated { .. } => {
                trace!("Ignoring accessibility updated event");
                Ok(())
            },
        }
    }

    pub fn position_window(
        &self,
        webview_window: &tao::window::Window,
        _window_id: &WindowId,
        position: Position,
    ) -> wry::Result<()> {
        webview_window.set_outer_position(position);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_cursor_position(&self) -> Option<Rect<i32, i32>> {
        None
    }

    pub fn get_active_window(&self) -> Option<PlatformWindow> {
        None
    }

    pub fn icon_lookup(_asset: &AssetSpecifier) -> Option<ProcessedAsset> {
        None
    }

    pub fn shell() -> Cow<'static, str> {
        "bash".into()
    }

    pub fn accessibility_is_enabled() -> Option<bool> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
enum ConsoleState {
    None,
    Console { hwnd: HWND },
    Accessible { x: i32, y: i32, width: i32, height: i32 },
}

struct Unmanaged {
    event_sender: Option<EventLoopProxy>,
    location_hook: Option<HWINEVENTHOOK>,
    console_state: ConsoleState,
}

#[derive(Debug)]
#[repr(C)]
struct Automation(IUIAutomation);
unsafe impl Sync for Automation {}
unsafe impl Send for Automation {}

#[derive(Debug)]
#[repr(C)]
struct AutomationEventHandler(IUIAutomationFocusChangedEventHandler);
unsafe impl Sync for AutomationEventHandler {}
unsafe impl Send for AutomationEventHandler {}

#[derive(Debug)]
#[implement(IUIAutomationFocusChangedEventHandler)]
#[repr(C)]
struct FocusChangedEventHandler {
    event_loop_proxy: EventLoopProxy,
}

impl IUIAutomationFocusChangedEventHandler_Impl for FocusChangedEventHandler {
    fn HandleFocusChangedEvent(
        &self,
        sender: &core::option::Option<windows::Win32::UI::Accessibility::IUIAutomationElement>,
    ) -> windows::core::Result<()> {
        if let Some(sender) = sender {
            unsafe {
                if let Ok(control_type) = sender.CurrentLocalizedControlType() {
                    if control_type == "terminal" {
                        self.event_loop_proxy
                            .send_event(Event::WindowEvent {
                                window_id: AUTOCOMPLETE_ID,
                                window_event: WindowEvent::Hide,
                            })
                            .ok();
                    }
                }
            }
        }

        Ok(())
    }
}

unsafe fn update_focused_state(hwnd: HWND) {
    let mut unmanaged = UNMANAGED.lock();

    if let Some(hook) = unmanaged.location_hook.take() {
        UnhookWinEvent(hook);
    }

    unmanaged
        .event_sender
        .as_ref()
        .unwrap()
        .send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID,
            window_event: WindowEvent::Hide,
        })
        .ok();

    let mut process_id = 0;
    let thread_id = GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    let process_handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
        Ok(process_handle) => process_handle,
        Err(e) => {
            debug!("Failed to get a handle to a Windows process, it's likely been closed: {e}");
            return;
        },
    };

    // Get the terminal name
    let mut process_name = vec![0; 256];
    let len = K32GetProcessImageFileNameA(process_handle, &mut process_name) as usize;
    process_name.truncate(len + 1);
    let title = match CStr::from_bytes_with_nul(&process_name)
        .expect("Missing null terminator")
        .to_str()
    {
        Ok(process_name) => match process_name.split('\\').last() {
            Some(title) => match title.strip_suffix(".exe") {
                Some(title) => title,
                None => return,
            },
            None => return,
        },
        Err(_) => return,
    };

    match title {
        title if ["Hyper", "Code", "Code - Insiders"].contains(&title) => (),
        title
            if [
                "bash",
                "cmd",
                "mintty",
                "powershell",
                "ubuntu2004",
                "ubuntu2204",
                "WindowsTerminal",
            ]
            .contains(&title) =>
        {
            unmanaged.console_state = ConsoleState::Console { hwnd }
        },
        _ => {
            unmanaged.console_state = ConsoleState::None;
            return;
        },
    }

    unmanaged.location_hook.replace(SetWinEventHook(
        EVENT_OBJECT_LOCATIONCHANGE,
        EVENT_OBJECT_LOCATIONCHANGE,
        None,
        Some(win_event_proc),
        process_id,
        thread_id,
        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
    ));
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _id_event_thread: u32,
    _time: u32,
) {
    match event {
        e if e == EVENT_SYSTEM_FOREGROUND
            && OBJECT_IDENTIFIER(id_object) == OBJID_WINDOW
            && id_child == CHILDID_SELF as i32 =>
        {
            update_focused_state(hwnd)
        },
        e if e == EVENT_OBJECT_LOCATIONCHANGE
            && OBJECT_IDENTIFIER(id_object) == OBJID_WINDOW
            && id_child == CHILDID_SELF as i32 =>
        {
            UNMANAGED
                .lock()
                .event_sender
                .as_ref()
                .unwrap()
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID,
                    window_event: WindowEvent::Hide,
                })
                .ok();
        },
        e if e == EVENT_OBJECT_LOCATIONCHANGE && OBJECT_IDENTIFIER(id_object) == OBJID_CARET => {
            let mut acc = None;
            let mut varchild = VARIANT::default();
            if AccessibleObjectFromEvent(hwnd, id_object as u32, id_child as u32, &mut acc, &mut varchild).is_ok() {
                if let Some(acc) = acc {
                    let mut left = 0;
                    let mut top = 0;
                    let mut width = 0;
                    let mut height = 0;
                    if acc
                        .accLocation(&mut left, &mut top, &mut width, &mut height, &varchild)
                        .is_ok()
                    {
                        UNMANAGED.lock().console_state = ConsoleState::Accessible {
                            x: left,
                            y: top,
                            width,
                            height,
                        }
                    }
                }
            }
        },
        _ => (),
    }
}

pub const fn autocomplete_active() -> bool {
    true
}
