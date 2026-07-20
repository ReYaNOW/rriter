use super::SystemProxyConfig;
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::io;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

const KEYCHAIN_SERVICE: &str = "com.rriter.RRiter";
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;

type SecKeychainItemRef = *mut c_void;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecKeychainFindGenericPassword(
        keychain_or_array: *const c_void,
        service_name_length: c_uint,
        service_name: *const c_char,
        account_name_length: c_uint,
        account_name: *const c_char,
        password_length: *mut c_uint,
        password_data: *mut *mut c_void,
        item_ref: *mut SecKeychainItemRef,
    ) -> c_int;
    fn SecKeychainAddGenericPassword(
        keychain: *const c_void,
        service_name_length: c_uint,
        service_name: *const c_char,
        account_name_length: c_uint,
        account_name: *const c_char,
        password_length: c_uint,
        password_data: *const c_void,
        item_ref: *mut SecKeychainItemRef,
    ) -> c_int;
    fn SecKeychainItemModifyAttributesAndData(
        item_ref: SecKeychainItemRef,
        attr_list: *const c_void,
        length: c_uint,
        data: *const c_void,
    ) -> c_int;
    fn SecKeychainItemDelete(item_ref: SecKeychainItemRef) -> c_int;
    fn SecKeychainItemFreeContent(attr_list: *const c_void, data: *mut c_void) -> c_int;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MachTimeValue {
    seconds: i32,
    microseconds: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MachTaskBasicInfo {
    virtual_size: u64,
    resident_size: u64,
    resident_size_max: u64,
    user_time: MachTimeValue,
    system_time: MachTimeValue,
    policy: i32,
    suspend_count: i32,
}

#[link(name = "System")]
unsafe extern "C" {
    static mach_task_self_: c_uint;
    fn task_info(
        target_task: c_uint,
        flavor: c_uint,
        task_info_out: *mut c_int,
        task_info_out_count: *mut c_uint,
    ) -> c_int;
}

fn os_status(status: i32, operation: &str) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "macOS {operation} failed with OSStatus {status}"
        )))
    }
}

fn keychain_length(value: usize, field: &str) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Keychain {field} is too large"),
        )
    })
}

pub fn store_keychain_secret(purpose: &str, bytes: &[u8]) -> io::Result<()> {
    let service = KEYCHAIN_SERVICE.as_bytes();
    let account = purpose.as_bytes();
    let service_length = keychain_length(service.len(), "service name")?;
    let account_length = keychain_length(account.len(), "account name")?;
    let secret_length = keychain_length(bytes.len(), "secret")?;
    let mut password_length = 0_u32;
    let mut password_data = std::ptr::null_mut();
    let mut item_ref = std::ptr::null_mut();
    let status = unsafe {
        SecKeychainFindGenericPassword(
            std::ptr::null(),
            service_length,
            service.as_ptr().cast(),
            account_length,
            account.as_ptr().cast(),
            &mut password_length,
            &mut password_data,
            &mut item_ref,
        )
    };
    if !password_data.is_null() {
        unsafe {
            let _ = SecKeychainItemFreeContent(std::ptr::null(), password_data);
        }
    }

    if status == 0 {
        let update_status = unsafe {
            SecKeychainItemModifyAttributesAndData(
                item_ref,
                std::ptr::null(),
                secret_length,
                bytes.as_ptr().cast(),
            )
        };
        if !item_ref.is_null() {
            unsafe { CFRelease(item_ref) };
        }
        return os_status(update_status, "Keychain update");
    }
    if !item_ref.is_null() {
        unsafe { CFRelease(item_ref) };
    }
    if status != ERR_SEC_ITEM_NOT_FOUND {
        return os_status(status, "Keychain lookup");
    }

    let add_status = unsafe {
        SecKeychainAddGenericPassword(
            std::ptr::null(),
            service_length,
            service.as_ptr().cast(),
            account_length,
            account.as_ptr().cast(),
            secret_length,
            bytes.as_ptr().cast(),
            std::ptr::null_mut(),
        )
    };
    os_status(add_status, "Keychain insert")
}

pub fn is_keychain_item_not_found(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}

pub fn load_keychain_secret(purpose: &str) -> io::Result<Vec<u8>> {
    let service = KEYCHAIN_SERVICE.as_bytes();
    let account = purpose.as_bytes();
    let service_length = keychain_length(service.len(), "service name")?;
    let account_length = keychain_length(account.len(), "account name")?;
    let mut password_length = 0_u32;
    let mut password_data = std::ptr::null_mut();
    let status = unsafe {
        SecKeychainFindGenericPassword(
            std::ptr::null(),
            service_length,
            service.as_ptr().cast(),
            account_length,
            account.as_ptr().cast(),
            &mut password_length,
            &mut password_data,
            std::ptr::null_mut(),
        )
    };
    if status == ERR_SEC_ITEM_NOT_FOUND {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Keychain item not found",
        ));
    }
    os_status(status, "Keychain lookup")?;
    if password_data.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Keychain returned an empty data pointer",
        ));
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(password_data.cast::<u8>(), password_length as usize).to_vec()
    };
    unsafe {
        let _ = SecKeychainItemFreeContent(std::ptr::null(), password_data);
    }
    Ok(bytes)
}

pub fn delete_keychain_secret(purpose: &str) -> io::Result<()> {
    let service = KEYCHAIN_SERVICE.as_bytes();
    let account = purpose.as_bytes();
    let service_length = keychain_length(service.len(), "service name")?;
    let account_length = keychain_length(account.len(), "account name")?;
    let mut item_ref = std::ptr::null_mut();
    let status = unsafe {
        SecKeychainFindGenericPassword(
            std::ptr::null(),
            service_length,
            service.as_ptr().cast(),
            account_length,
            account.as_ptr().cast(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut item_ref,
        )
    };
    if status == ERR_SEC_ITEM_NOT_FOUND {
        return Ok(());
    }
    os_status(status, "Keychain lookup")?;
    if item_ref.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Keychain returned an empty item reference",
        ));
    }
    let delete_status = unsafe { SecKeychainItemDelete(item_ref) };
    unsafe { CFRelease(item_ref) };
    os_status(delete_status, "Keychain delete")
}

pub fn current_process_memory_kb() -> Option<usize> {
    const MACH_TASK_BASIC_INFO: u32 = 20;
    let mut info = MachTaskBasicInfo::default();
    let mut count =
        (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<c_int>()) as u32;
    let status = unsafe {
        task_info(
            mach_task_self_,
            MACH_TASK_BASIC_INFO,
            (&raw mut info).cast(),
            &mut count,
        )
    };
    (status == 0).then_some((info.resident_size / 1024) as usize)
}

pub fn open_url(url: &str) -> io::Result<()> {
    let status = Command::new("/usr/bin/open").arg(url).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "open exited with status {status}"
        )))
    }
}

pub(super) fn run_elevated_helper(executable: &Path, request: &Path) -> io::Result<i32> {
    const SCRIPT: &str = r#"on run argv
set executablePath to item 1 of argv
set requestPath to item 2 of argv
do shell script quoted form of executablePath & " --rriter-elevated-save " & quoted form of requestPath with administrator privileges
end run"#;

    let output = Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT, "--"])
        .arg(executable)
        .arg(request)
        .output()?;
    if output.status.success() {
        return Ok(0);
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.contains("User canceled") || message.contains("(-128)") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "macOS administrator authorization was cancelled",
        ));
    }
    if !message.is_empty() {
        eprintln!("macOS elevated helper reported: {message}");
    }
    Ok(output.status.code().unwrap_or(1))
}

pub fn reveal_path(path: &Path) -> io::Result<Child> {
    let mut command = Command::new("/usr/bin/open");
    if path.is_file() {
        command.arg("-R");
    }
    command.arg(path).spawn()
}

pub fn system_proxy_config() -> Option<SystemProxyConfig> {
    let mut command = Command::new("/usr/sbin/scutil");
    command.arg("--proxy");
    let output = super::run_command_output(&mut command, Duration::from_secs(3)).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(|output| super::integration::parse_macos_proxy_config(&output))
}

pub fn native_root_certificates_der() -> io::Result<Vec<Vec<u8>>> {
    let mut command = Command::new("/usr/bin/security");
    command.args(["find-certificate", "-a", "-p"]);
    let output = super::run_command_output(&mut command, Duration::from_secs(10))?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(super::integration::parse_pem_certificates(&output.stdout))
}
