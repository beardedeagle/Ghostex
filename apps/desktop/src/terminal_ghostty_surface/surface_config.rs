use std::{
    ffi::{CString, c_void},
    fmt,
    ptr::{self, NonNull},
};

use crate::ghostty_kit::ffi;

use super::*;
#[cfg(target_os = "macos")]
use crate::terminal_native_view::RealTerminalNativeViewHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceNsViewHandle {
    nsview: NonNull<c_void>,
}

impl GhosttySurfaceNsViewHandle {
    /// # Safety
    ///
    /// `nsview` must be an existing real AppKit `NSView` that remains valid until the eventual
    /// Ghostty surface config consumer finishes using the produced FFI struct.
    #[allow(dead_code)] // no caller: the surface host owns NSView creation now
    pub(crate) unsafe fn from_existing_nsview(nsview: NonNull<c_void>) -> Self {
        Self { nsview }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn from_terminal_native_view(native_view: RealTerminalNativeViewHandle) -> Self {
        Self {
            nsview: native_view.as_non_null(),
        }
    }

    pub(crate) fn as_ptr(self) -> *mut c_void {
        self.nsview.as_ptr()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GhosttySurfaceScaleFactor(f64);

impl GhosttySurfaceScaleFactor {
    pub(crate) fn new(scale_factor: f64) -> Result<Self, GhosttySurfaceConfigRequestError> {
        if scale_factor.is_finite() && scale_factor > 0.0 {
            Ok(Self(scale_factor))
        } else {
            Err(GhosttySurfaceConfigRequestError::InvalidScaleFactor(
                scale_factor,
            ))
        }
    }

    pub(crate) fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GhosttySurfaceConfigRequestError {
    InvalidScaleFactor(f64),
    LaunchPayloadContainsInteriorNul {
        field: GhosttySurfaceLaunchPayloadField,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GhosttySurfaceLaunchPayloadField {
    WorkingDirectory,
    Command,
    EnvVarKey,
    EnvVarValue,
    InitialInput,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceLaunchEnvVar {
    key: String,
    value: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceLaunchPayload {
    working_directory: Option<String>,
    command: Option<String>,
    env_vars: Vec<GhosttySurfaceLaunchEnvVar>,
    initial_input: Option<String>,
    wait_after_command: bool,
}

impl GhosttySurfaceLaunchPayload {
    pub(crate) fn try_new(
        working_directory: Option<String>,
        command: Option<String>,
        env_vars: Vec<(String, String)>,
        initial_input: Option<String>,
        wait_after_command: bool,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::WorkingDirectory,
            working_directory.as_deref(),
        )?;
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::Command,
            command.as_deref(),
        )?;
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::InitialInput,
            initial_input.as_deref(),
        )?;

        let env_vars = crate::terminal_environment::color_capable_terminal_env_vars(env_vars)
            .into_iter()
            .map(|(key, value)| {
                validate_launch_string(GhosttySurfaceLaunchPayloadField::EnvVarKey, &key)?;
                validate_launch_string(GhosttySurfaceLaunchPayloadField::EnvVarValue, &value)?;
                Ok(GhosttySurfaceLaunchEnvVar { key, value })
            })
            .collect::<Result<Vec<_>, GhosttySurfaceConfigRequestError>>()?;

        Ok(Self {
            working_directory,
            command,
            env_vars,
            initial_input,
            wait_after_command,
        })
    }

    fn env_var_count(&self) -> usize {
        self.env_vars.len()
    }
}

impl fmt::Debug for GhosttySurfaceLaunchPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GhosttySurfaceLaunchPayload")
            .field("has_working_directory", &self.working_directory.is_some())
            .field("has_command", &self.command.is_some())
            .field("env_var_count", &self.env_var_count())
            .field("has_initial_input", &self.initial_input.is_some())
            .field("wait_after_command", &self.wait_after_command)
            .finish()
    }
}

fn validate_optional_launch_string(
    field: GhosttySurfaceLaunchPayloadField,
    value: Option<&str>,
) -> Result<(), GhosttySurfaceConfigRequestError> {
    if let Some(value) = value {
        validate_launch_string(field, value)?;
    }
    Ok(())
}

fn validate_launch_string(
    field: GhosttySurfaceLaunchPayloadField,
    value: &str,
) -> Result<(), GhosttySurfaceConfigRequestError> {
    if value.as_bytes().contains(&0) {
        Err(GhosttySurfaceConfigRequestError::LaunchPayloadContainsInteriorNul { field })
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GhosttySurfaceTerminalConfig {
    font_size: f32,
}

impl GhosttySurfaceTerminalConfig {
    pub(crate) fn unmanaged() -> Self {
        Self { font_size: 0.0 }
    }

    pub(crate) fn with_font_size(font_size: f32) -> Self {
        Self { font_size }
    }

    fn font_size(self) -> f32 {
        self.font_size
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct GhosttySurfaceConfigRequest {
    nsview: GhosttySurfaceNsViewHandle,
    scale_factor: GhosttySurfaceScaleFactor,
    terminal_config: GhosttySurfaceTerminalConfig,
    launch_payload: Option<GhosttySurfaceLaunchPayload>,
}

impl GhosttySurfaceConfigRequest {
    pub(crate) fn new(
        nsview: GhosttySurfaceNsViewHandle,
        scale_factor: GhosttySurfaceScaleFactor,
    ) -> Self {
        Self {
            nsview,
            scale_factor,
            terminal_config: GhosttySurfaceTerminalConfig::unmanaged(),
            launch_payload: None,
        }
    }

    pub(crate) fn try_new(
        nsview: GhosttySurfaceNsViewHandle,
        scale_factor: f64,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        Ok(Self::new(
            nsview,
            GhosttySurfaceScaleFactor::new(scale_factor)?,
        ))
    }

    pub(crate) fn with_launch_payload(
        mut self,
        launch_payload: GhosttySurfaceLaunchPayload,
    ) -> Self {
        self.launch_payload = Some(launch_payload);
        self
    }

    pub(crate) fn with_terminal_config(
        mut self,
        terminal_config: GhosttySurfaceTerminalConfig,
    ) -> Self {
        self.terminal_config = terminal_config;
        self
    }

    pub(crate) fn set_terminal_config(&mut self, terminal_config: GhosttySurfaceTerminalConfig) {
        self.terminal_config = terminal_config;
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn try_from_terminal_native_view(
        native_view: RealTerminalNativeViewHandle,
        scale_factor: f64,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        Self::try_new(
            GhosttySurfaceNsViewHandle::from_terminal_native_view(native_view),
            scale_factor,
        )
    }

    #[allow(dead_code)] // no caller: the live path builds the FFI surface config through the surface host
    pub(crate) fn to_ffi_config(&self) -> ffi::ghostty_surface_config_s {
        assert!(
            self.launch_payload.is_none(),
            "launch-bearing Ghostty configs require scoped preparation"
        );
        let mut config = empty_ffi_surface_config();
        self.apply_base_to_ffi_config(&mut config);
        config
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        self.scale_factor.get()
    }

    pub(crate) fn prepare_ffi_config(
        &self,
        mut config: ffi::ghostty_surface_config_s,
    ) -> GhosttySurfacePreparedConfig {
        self.apply_base_to_ffi_config(&mut config);
        GhosttySurfacePreparedConfig::new(config, self.launch_payload.as_ref())
    }

    fn apply_base_to_ffi_config(&self, config: &mut ffi::ghostty_surface_config_s) {
        /*
        CDXC:Terminal 2026-06-27-10:10:
        Embedded Ghostty surface requests can carry only the GhosttyKit-supported live/recreate FFI typography field, `font_size`; config-file-backed settings such as font family, theme, cursor, scrollback, clipboard, and mouse are intentionally not represented in `ghostty_surface_config_s`. A `font_size` of 0.0 remains the unmanaged Ghostty default for generic callers, while GPUI-owned request builders attach the shared Settings `terminalFontSize` value before creating Agents, command, or startup surfaces; live surface reload is not claimed here.
        */
        let nsview = self.nsview.as_ptr();

        config.platform_tag = ffi::GHOSTTY_PLATFORM_MACOS;
        config.platform = ffi::ghostty_platform_u {
            macos: ffi::ghostty_platform_macos_s { nsview },
        };
        config.userdata = nsview;
        config.scale_factor = self.scale_factor.get();
        config.font_size = self.terminal_config.font_size();
        config.working_directory = ptr::null();
        config.command = ptr::null();
        config.env_vars = ptr::null_mut();
        config.env_var_count = 0;
        config.initial_input = ptr::null();
        config.wait_after_command = false;
        config.context = ffi::GHOSTTY_SURFACE_CONTEXT_WINDOW;
    }
}

impl fmt::Debug for GhosttySurfaceConfigRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GhosttySurfaceConfigRequest")
            .field("scale_factor", &self.scale_factor.get())
            .field("has_launch_payload", &self.launch_payload.is_some())
            .field(
                "launch_env_var_count",
                &self
                    .launch_payload
                    .as_ref()
                    .map_or(0, GhosttySurfaceLaunchPayload::env_var_count),
            )
            .finish()
    }
}

pub(crate) struct GhosttySurfacePreparedConfig {
    config: ffi::ghostty_surface_config_s,
    _working_directory: Option<CString>,
    _command: Option<CString>,
    _env_keys: Vec<CString>,
    _env_values: Vec<CString>,
    _env_vars: Vec<ffi::ghostty_env_var_s>,
    _initial_input: Option<CString>,
}

impl GhosttySurfacePreparedConfig {
    fn new(
        mut config: ffi::ghostty_surface_config_s,
        launch_payload: Option<&GhosttySurfaceLaunchPayload>,
    ) -> Self {
        let Some(launch_payload) = launch_payload else {
            return Self {
                config,
                _working_directory: None,
                _command: None,
                _env_keys: Vec::new(),
                _env_values: Vec::new(),
                _env_vars: Vec::new(),
                _initial_input: None,
            };
        };

        let working_directory = launch_payload
            .working_directory
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let command = launch_payload
            .command
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let initial_input = launch_payload
            .initial_input
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let env_keys = launch_payload
            .env_vars
            .iter()
            .map(|env_var| cstring_from_validated_launch_string(&env_var.key))
            .collect::<Vec<_>>();
        let env_values = launch_payload
            .env_vars
            .iter()
            .map(|env_var| cstring_from_validated_launch_string(&env_var.value))
            .collect::<Vec<_>>();
        let mut env_vars = env_keys
            .iter()
            .zip(env_values.iter())
            .map(|(key, value)| ffi::ghostty_env_var_s {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect::<Vec<_>>();

        config.working_directory = working_directory
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        config.command = command.as_ref().map_or(ptr::null(), |value| value.as_ptr());
        config.initial_input = initial_input
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        config.wait_after_command = launch_payload.wait_after_command;
        config.env_var_count = env_vars.len();
        config.env_vars = if env_vars.is_empty() {
            ptr::null_mut()
        } else {
            env_vars.as_mut_ptr()
        };

        Self {
            config,
            _working_directory: working_directory,
            _command: command,
            _env_keys: env_keys,
            _env_values: env_values,
            _env_vars: env_vars,
            _initial_input: initial_input,
        }
    }

    pub(crate) fn as_ptr(&self) -> *const ffi::ghostty_surface_config_s {
        &self.config
    }

    pub(crate) fn set_surface_userdata(&mut self, userdata: *mut c_void) {
        self.config.userdata = userdata;
    }
}

fn cstring_from_validated_launch_string(value: &str) -> CString {
    CString::new(value).expect("launch payload strings are validated before FFI preparation")
}

fn empty_ffi_surface_config() -> ffi::ghostty_surface_config_s {
    ffi::ghostty_surface_config_s {
        platform_tag: ffi::GHOSTTY_PLATFORM_INVALID,
        platform: ffi::ghostty_platform_u {
            macos: ffi::ghostty_platform_macos_s {
                nsview: ptr::null_mut(),
            },
        },
        userdata: ptr::null_mut(),
        scale_factor: 1.0,
        font_size: 0.0,
        working_directory: ptr::null(),
        command: ptr::null(),
        env_vars: ptr::null_mut(),
        env_var_count: 0,
        initial_input: ptr::null(),
        wait_after_command: false,
        context: ffi::GHOSTTY_SURFACE_CONTEXT_WINDOW,
    }
}

pub(crate) struct GhosttyConfigOwner {
    pub(crate) config: NonNull<c_void>,
    pub(crate) functions: GhosttyKitFunctionTable,
}

impl GhosttyConfigOwner {
    /// Ghostex's own terminal config, never Ghostty's default files: those are the Ghostty app's
    /// config (`~/.config/ghostty`, `com.mitchellh.ghostty`), which Ghostex no longer reads.
    pub(crate) fn load_ghostex_terminal_config_finalized_with_functions(
        functions: GhosttyKitFunctionTable,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let path = crate::shared_settings::ghostex_terminal_config_path();
        let path = CString::new(path.to_string_lossy().into_owned())
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigPathContainsInteriorNul)?;
        let config = unsafe { (functions.config_new)() };
        let config =
            NonNull::new(config).ok_or(GhosttySurfaceRuntimeError::ConfigCreateReturnedNull)?;
        let owner = Self { config, functions };

        unsafe {
            // Give native macOS surfaces the GPUI keymap's clipboard alias,
            // seeding it before user bindings take precedence.
            #[cfg(target_os = "macos")]
            {
                let alias = "keybind = shift+insert=paste_from_clipboard\n";
                (functions.config_load_string)(owner.as_raw(), alias.as_ptr().cast(), alias.len());
            }
            (functions.config_load_file)(owner.as_raw(), path.as_ptr());
            (functions.config_load_recursive_files)(owner.as_raw());
            (functions.config_finalize)(owner.as_raw());
        }

        Ok(owner)
    }

    pub(crate) fn as_raw(&self) -> ffi::ghostty_config_t {
        self.config.as_ptr()
    }
}

impl Drop for GhosttyConfigOwner {
    fn drop(&mut self) {
        unsafe {
            (self.functions.config_free)(self.as_raw());
        }
    }
}
