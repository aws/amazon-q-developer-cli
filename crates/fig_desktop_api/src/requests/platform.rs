use fig_os_shim::{
    EnvProvider,
    PlatformProvider,
};
use fig_proto::fig::{
    AppBundleType,
    DesktopEnvironment,
    DisplayServerProtocol,
    GetPlatformInfoRequest,
    GetPlatformInfoResponse,
    Os,
};
use fig_util::system_info::linux::{
    DesktopEnvironment as FigDesktopEnvironment,
    DisplayServer,
    get_desktop_environment,
    get_display_server,
};

use super::{
    Error,
    RequestResult,
    ServerOriginatedSubMessage,
};

pub async fn get_platform_info<Ctx>(_request: GetPlatformInfoRequest, ctx: &Ctx) -> RequestResult
where
    Ctx: EnvProvider + PlatformProvider,
{
    let os = match ctx.platform().os() {
        fig_os_shim::Os::Mac => Os::Macos,
        fig_os_shim::Os::Linux => Os::Linux,
        _ => return Err("Unsupported operating system".into()),
    };
    let desktop_environment = if os == Os::Linux {
        match get_desktop_environment(ctx).map_err(Error::from_std)? {
            FigDesktopEnvironment::Gnome => Some(DesktopEnvironment::Gnome.into()),
            _ => None,
        }
    } else {
        None
    };
    let display_server_protocol = if os == Os::Linux {
        match get_display_server(ctx).map_err(Error::from_std)? {
            DisplayServer::X11 => Some(DisplayServerProtocol::X11.into()),
            DisplayServer::Wayland => Some(DisplayServerProtocol::Wayland.into()),
        }
    } else {
        None
    };
    let app_bundle_type = if os == Os::Linux && ctx.env().in_appimage() {
        Some(AppBundleType::Appimage.into())
    } else {
        None
    };

    let response = ServerOriginatedSubMessage::GetPlatformInfoResponse(GetPlatformInfoResponse {
        os: os.into(),
        desktop_environment,
        display_server_protocol,
        app_bundle_type,
    });
    Ok(response.into())
}
