use super::{identity::{self, Process}, wide, win, Handle};
use anyhow::{ensure, Context, Result};
use std::{fs::{File, OpenOptions}, mem::{size_of, zeroed}, os::windows::{fs::OpenOptionsExt, io::FromRawHandle}, path::Path, ptr, sync::Arc};
use windows_sys::Win32::{Foundation::*, Security::*, Storage::FileSystem::*, System::{Console::*, Environment::*, JobObjects::*, Pipes::CreatePipe, Threading::*}, UI::{Shell::*, WindowsAndMessaging::SW_HIDE}};

pub fn duplicate(raw: HANDLE, inherit: bool) -> Result<Handle> {
    let mut result = ptr::null_mut();
    unsafe { win(DuplicateHandle(GetCurrentProcess(), raw, GetCurrentProcess(), &mut result, 0, inherit.into(), DUPLICATE_SAME_ACCESS)).context("DuplicateHandle")?; }
    Handle::new(result)
}
pub fn file(handle: Handle) -> File { let raw = handle.raw(); std::mem::forget(handle); unsafe { File::from_raw_handle(raw) } }
fn anonymous_pipe() -> Result<(Handle, Handle)> {
    let mut read = ptr::null_mut(); let mut write = ptr::null_mut();
    unsafe { win(CreatePipe(&mut read, &mut write, ptr::null(), 65536))?; }
    Ok((Handle::new(read)?, Handle::new(write)?))
}
pub struct Child { pub process: Handle, _job: Handle, _image: File, _probe_excluded: Option<ParentHandle>, _probe_excluded_event: Option<Handle>, pub input: File, pub output: File, pub packet_handle: String }
pub fn lock_image(executable: &Path) -> Result<File> { Ok(OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(executable)?) }
fn terminate_process(process: HANDLE) -> Result<()> {
    if unsafe { WaitForSingleObject(process,0) } == WAIT_OBJECT_0 { return Ok(()); }
    let terminated = unsafe { TerminateProcess(process,1) };
    if terminated == 0 && unsafe {WaitForSingleObject(process,0)} != WAIT_OBJECT_0 { win(terminated)?; }
    ensure!(unsafe { WaitForSingleObject(process,5000) } == WAIT_OBJECT_0,"engine_termination_timeout"); Ok(())
}
impl Child {
    pub fn terminate(&self) -> Result<()> {
        terminate_process(self.process.raw())
    }
    pub fn verify_probe_exclusion(&self)->Result<()> {
        let event=self._probe_excluded_event.as_ref().ok_or_else(||anyhow::anyhow!("probe_event_required"))?;
        ensure!(unsafe {WaitForSingleObject(event.raw(),0)}==WAIT_TIMEOUT,"unlisted_parent_event_was_inherited");Ok(())
    }
}
struct Attributes { data: Vec<usize>, initialized: bool, _parent: Option<Box<HANDLE>> }
impl Attributes {
    fn new(handles: &mut [HANDLE]) -> Result<Self> {
        Self::with_parent(handles,None)
    }
    fn with_parent(handles: &mut [HANDLE], parent: Option<HANDLE>) -> Result<Self> {
        let mut size = 0;
        let count=if parent.is_some() {2} else {1};
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut size); }
        ensure!(size > 0 && size < 1024 * 1024, "invalid_process_attribute_size");
        let mut result = Self { data: vec![0; size.div_ceil(size_of::<usize>())], initialized: false, _parent:parent.map(Box::new) };
        unsafe { win(InitializeProcThreadAttributeList(result.data.as_mut_ptr().cast(), count, 0, &mut size)).context("InitializeProcThreadAttributeList")?; }
        result.initialized = true;
        unsafe { win(UpdateProcThreadAttribute(result.data.as_mut_ptr().cast(), 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize, handles.as_mut_ptr().cast(), std::mem::size_of_val(handles), ptr::null_mut(), ptr::null())).context("UpdateProcThreadAttribute_HANDLE_LIST")?; }
        if let Some(parent)=result._parent.as_mut() {unsafe {win(UpdateProcThreadAttribute(result.data.as_mut_ptr().cast(),0,PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,(&mut **parent as *mut HANDLE).cast(),size_of::<HANDLE>(),ptr::null_mut(),ptr::null())).context("UpdateProcThreadAttribute_PARENT_PROCESS")?;}}
        Ok(result)
    }
}
impl Drop for Attributes { fn drop(&mut self) { if self.initialized { unsafe { DeleteProcThreadAttributeList(self.data.as_mut_ptr().cast()); } } } }
struct Environment(*mut core::ffi::c_void);
impl Drop for Environment { fn drop(&mut self) { unsafe { DestroyEnvironmentBlock(self.0); } } }

/// This value belongs to the authenticated parent's handle table, never ours.
struct ParentHandle { parent: Arc<Handle>, raw: HANDLE }
// The value is an opaque remote-table entry, used only with thread-safe DuplicateHandle.
unsafe impl Send for ParentHandle {}
impl ParentHandle {
    fn duplicate(parent:&Arc<Handle>,source:HANDLE)->Result<Self> {
        let mut raw=ptr::null_mut();
        unsafe {win(DuplicateHandle(GetCurrentProcess(),source,parent.raw(),&mut raw,0,1,DUPLICATE_SAME_ACCESS)).context("DuplicateHandle_into_authenticated_parent")?;}
        Ok(Self {parent:parent.clone(),raw})
    }
    fn close(&mut self)->Result<()> {
        if self.raw.is_null() {return Ok(());}
        let raw=std::mem::replace(&mut self.raw,ptr::null_mut());let mut local=ptr::null_mut();
        // CLOSE_SOURCE closes the source even if duplication reports an error.
        unsafe {win(DuplicateHandle(self.parent.raw(),raw,GetCurrentProcess(),&mut local,0,0,DUPLICATE_SAME_ACCESS|DUPLICATE_CLOSE_SOURCE)).context("DuplicateHandle_close_authenticated_parent_copy")?;}
        drop(Handle::new(local)?);Ok(())
    }
}
impl Drop for ParentHandle {fn drop(&mut self) {if let Err(error)=self.close() {eprintln!("parent_handle_cleanup: {error:#}");}}}

fn launch_parent(owner:&Process)->Result<Handle> {
    owner.verify(owner.pid)?;
    let parent=Handle::new(unsafe {OpenProcess(PROCESS_CREATE_PROCESS|PROCESS_DUP_HANDLE|PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE,0,owner.pid)}).context("OpenProcess_authenticated_engine_parent")?;
    ensure!(unsafe {GetProcessId(parent.raw())}==owner.pid && identity::birth_time(parent.raw())?==owner.birth,"engine_parent_pid_reused");
    ensure!(unsafe {WaitForSingleObject(parent.raw(),0)}==WAIT_TIMEOUT,"engine_parent_exited");
    let actual=identity::process_identity(parent.raw())?;identity::same_logon(&owner.identity,&actual)?;ensure!(!actual.elevated,"engine_parent_must_be_unelevated");Ok(parent)
}

/// Spawn only a fixed sibling engine with the original manager's unelevated token.
/// The suspended child joins its kill-on-close job before any engine code runs.
pub fn engine(owner: &Process, executable: &Path, packet: HANDLE) -> Result<Child> {
    ensure!(!owner.identity.elevated, "vpn_manager_must_be_unelevated");
    ensure!(executable.is_absolute() && executable.file_name().is_some_and(|v| v == "termterm-openvpn.exe" || v == "termterm-openconnect.exe" || v == "termterm-wireguard.exe"), "fixed_engine_executable_required");
    spawn_engine_child(owner,executable,packet,None)
}
/// The debug desktop uses the same private-pipe/job launcher without a privileged broker.
#[cfg(all(feature="auth-only-test", debug_assertions))]
pub fn auth_engine(executable: &Path) -> Result<Child> {
    ensure!(executable.file_name().is_some_and(|name| name == "termterm-openconnect.exe"), "fixed_auth_engine_required");
    let owner = Process::open(std::process::id())?;
    // Auth-only exits before packet setup; an event preserves the launcher's handle whitelist.
    let unused = Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
    engine(&owner, executable, unused.raw())
}
fn spawn_engine_child(owner:&Process,executable:&Path,packet:HANDLE,probe:Option<&str>)->Result<Child> {
    ensure!(!owner.identity.elevated,"vpn_manager_must_be_unelevated");
    let image = lock_image(executable).context("lock_engine_image")?;
    let current = identity::process_identity(unsafe {GetCurrentProcess()})?;
    if !current.elevated {ensure!(owner.pid == std::process::id(),"unelevated_spawn_requires_current_owner");identity::same_logon(&current,&owner.identity)?;}
    let parent=Arc::new(launch_parent(owner)?);
    let mut token = ptr::null_mut();
    unsafe { win(OpenProcessToken(owner.handle.raw(), TOKEN_QUERY | TOKEN_DUPLICATE, &mut token)).context("OpenProcessToken_engine_manager_environment")?; }
    let token = Handle::new(token)?;
    identity::same_logon(&owner.identity, &identity::from_token(token.raw())?)?;
    let (input_read, input_write) = anonymous_pipe()?; let (output_read, output_write) = anonymous_pipe()?;
    let mut child_input = ParentHandle::duplicate(&parent,input_read.raw())?; let mut child_output = ParentHandle::duplicate(&parent,output_write.raw())?;
    let mut child_packet = ParentHandle::duplicate(&parent,packet)?;
    let null = Handle::new(unsafe { CreateFileW(wide("NUL")?.as_ptr(), GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, ptr::null_mut()) })?;
    let mut child_error = ParentHandle::duplicate(&parent,null.raw())?;
    let (excluded,excluded_event)=if probe.is_some() {
        let event=Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
        (Some(ParentHandle::duplicate(&parent,event.raw())?),Some(event))
    } else {(None,None)};
    let mut inherited = [child_input.raw, child_output.raw, child_error.raw, child_packet.raw];
    let mut attributes = Attributes::with_parent(&mut inherited,Some(parent.raw()))?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32; startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = child_input.raw; startup.StartupInfo.hStdOutput = child_output.raw; startup.StartupInfo.hStdError = child_error.raw;
    startup.lpAttributeList = attributes.data.as_mut_ptr().cast();
    let mut environment = Environment(ptr::null_mut());
    unsafe { win(CreateEnvironmentBlock(&mut environment.0, token.raw(), 0)).context("CreateEnvironmentBlock_engine_manager")?; }
    let job = Handle::new(unsafe { CreateJobObjectW(ptr::null(), ptr::null()) })?;
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    unsafe { win(SetInformationJobObject(job.raw(), JobObjectExtendedLimitInformation, (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(), size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32)).context("SetInformationJobObject_engine")?; }
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    let packet_value=format!("{}",child_packet.raw as usize);
    let application = wide(executable.as_os_str())?;
    let mut command = wide(match probe {Some(scenario)=>format!("\"{}\" --parent-launch-probe --handle {packet_value} --parent {} --birth {} --excluded {} --scenario {scenario}",executable.display(),owner.pid,owner.birth,excluded.as_ref().unwrap().raw as usize),None=>format!("\"{}\"",executable.display())})?;
    let directory = wide(executable.parent().ok_or_else(|| anyhow::anyhow!("engine_parent_missing"))?.as_os_str())?;
    let flags = CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT;
    owner.verify(owner.pid)?;
    unsafe {win(CreateProcessW(application.as_ptr(),command.as_mut_ptr(),ptr::null(),ptr::null(),1,flags,environment.0,directory.as_ptr(),&startup.StartupInfo,&mut info)).context("CreateProcessW_authenticated_parent_engine")?;}
    let process = Handle::new(info.hProcess)?; let thread = Handle::new(info.hThread)?;
    let configured = (|| -> Result<()> {
        // No child instruction runs until job assignment and actual token proof succeed.
        unsafe { win(AssignProcessToJobObject(job.raw(), process.raw())).context("AssignProcessToJobObject_engine")?; }
        let actual = identity::process_identity(process.raw())?;
        identity::same_logon(&owner.identity, &actual)?; ensure!(!actual.elevated, "engine_must_not_be_elevated");
        for handle in [&mut child_input,&mut child_output,&mut child_error,&mut child_packet] {handle.close()?;}
        ensure!(unsafe { ResumeThread(thread.raw()) } != u32::MAX, "engine_resume_failed"); Ok(())
    })();
    if let Err(error) = configured { unsafe { TerminateProcess(process.raw(), 1); WaitForSingleObject(process.raw(), 5000); } return Err(error); }
    drop(input_read); drop(output_write);
    Ok(Child { process, _job: job, _image: image, _probe_excluded:excluded, _probe_excluded_event:excluded_event, input: file(input_write), output: file(output_read), packet_handle: packet_value })
}

/// No network APIs: the child proves the same suspended/job/parent/handle path used by engines.
pub fn parent_launch_probe(owner:&Process,helper:&Path,packet:HANDLE,scenario:&str)->Result<Child> {
    ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
    ensure!(matches!(scenario,"echo"|"job-close"),"invalid_parent_launch_probe_scenario");
    spawn_engine_child(owner,helper,packet,Some(scenario))
}

pub fn elevate(helper: &Path, pipe_id: uuid::Uuid, owner: &Process, validation: bool) -> Result<Process> {
    ensure!(helper.is_absolute() && helper.file_name().is_some_and(|v| v == "termterm-vpn-broker.exe"), "fixed_broker_executable_required");
    let verb = wide("runas")?; let application = wide(helper.as_os_str())?;
    ensure!(!validation || cfg!(feature="privileged-validation"),"validation_build_required");
    let action = if validation {"--serve-validation"} else {"--serve"};
    let parameters = wide(format!("{action} --pipe {pipe_id} --parent {} --birth {}", owner.pid, owner.birth))?;
    let mut execute: SHELLEXECUTEINFOW = unsafe { zeroed() };
    execute.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    execute.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    execute.lpVerb = verb.as_ptr(); execute.lpFile = application.as_ptr(); execute.lpParameters = parameters.as_ptr(); execute.nShow = SW_HIDE;
    unsafe { win(ShellExecuteExW(&mut execute))?; }
    let process = Handle::new(execute.hProcess)?; let pid = unsafe { GetProcessId(process.raw()) };
    let expected = Process::open(pid).map_err(|error|super::broker_startup_error(process.raw(),error))?;
    ensure!(identity::birth_time(process.raw())? == expected.birth, "launched_broker_pid_reused");
    identity::verify_elevated_peer(&owner.identity,expected.handle.raw())?;
    Ok(expected)
}

/// Watchdog inherits its wait/lock/ready handles, NUL stdio and diagnostic stderr only.
pub fn watchdog(helper: &Path, lock: &Handle) -> Result<Handle> {
    watchdog_spawn(helper,lock,unsafe {GetStdHandle(STD_ERROR_HANDLE)})
}
fn watchdog_spawn(helper: &Path, lock: &Handle, parent_error: HANDLE) -> Result<Handle> {
    let broker = duplicate(unsafe { GetCurrentProcess() }, true)?; let lock = duplicate(lock.raw(), true)?;
    let ready = Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
    let child_ready = duplicate(ready.raw(),true)?;
    let null=Handle::new(unsafe {CreateFileW(wide("NUL")?.as_ptr(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ|FILE_SHARE_WRITE,ptr::null(),OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,ptr::null_mut())})?;
    let child_null=duplicate(null.raw(),true)?;
    let diagnostic_error=cfg!(feature="privileged-validation") && !parent_error.is_null() && parent_error != INVALID_HANDLE_VALUE && unsafe {GetFileType(parent_error)} == FILE_TYPE_PIPE;
    let child_error=duplicate(if diagnostic_error {parent_error} else {null.raw()},true)?;
    let mut inherited = [broker.raw(), lock.raw(), child_ready.raw(),child_null.raw(),child_error.raw()]; let mut attributes = Attributes::new(&mut inherited)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() }; startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags=STARTF_USESTDHANDLES;startup.StartupInfo.hStdInput=child_null.raw();startup.StartupInfo.hStdOutput=child_null.raw();startup.StartupInfo.hStdError=child_error.raw();
    startup.lpAttributeList = attributes.data.as_mut_ptr().cast();
    let application = wide(helper.as_os_str())?;
    let mut command = wide(format!("\"{}\" --watchdog --broker-handle {} --lock-handle {} --ready-handle {}", helper.display(), broker.raw() as usize, lock.raw() as usize, child_ready.raw() as usize))?;
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    unsafe { win(CreateProcessW(application.as_ptr(), command.as_mut_ptr(), ptr::null(), ptr::null(), 1, EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW, ptr::null(), ptr::null(), &startup.StartupInfo, &mut info)).context("CreateProcessW_watchdog")?; }
    let process = Handle::new(info.hProcess)?; drop(Handle::new(info.hThread)?);
    let wait_handles = [ready.raw(),process.raw()];
    ensure!(unsafe {WaitForMultipleObjects(2,wait_handles.as_ptr(),0,10000)} == WAIT_OBJECT_0,"watchdog_not_ready");
    Ok(process)
}
pub struct Probe { pub process: Handle, _job: Handle, _image: File }
pub fn packet_probe(helper: &Path, endpoint: &Handle, scenario: &str) -> Result<Probe> {
    ensure!(matches!(scenario,"echo-cancel" | "oversize" | "eof"),"invalid_probe_scenario");
    ensure!(!identity::process_identity(unsafe {GetCurrentProcess()})?.elevated,"packet_probe_requires_unelevated_user");
    let image = lock_image(helper)?; let packet = duplicate(endpoint.raw(),true)?;
    let mut inherited = [packet.raw()]; let mut attributes = Attributes::new(&mut inherited)?;
    let mut startup: STARTUPINFOEXW = unsafe {zeroed()}; startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32; startup.lpAttributeList = attributes.data.as_mut_ptr().cast();
    let owner = Process::open(std::process::id())?;
    let application = wide(helper.as_os_str())?;
    let mut command = wide(format!("\"{}\" --packet-probe --handle {} --parent {} --birth {} --scenario {scenario}",helper.display(),packet.raw() as usize,owner.pid,owner.birth))?;
    let job = Handle::new(unsafe {CreateJobObjectW(ptr::null(),ptr::null())})?;
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe {zeroed()}; limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {win(SetInformationJobObject(job.raw(),JobObjectExtendedLimitInformation,(&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32))?;}
    let mut info: PROCESS_INFORMATION = unsafe {zeroed()};
    unsafe {win(CreateProcessW(application.as_ptr(),command.as_mut_ptr(),ptr::null(),ptr::null(),1,CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,ptr::null(),ptr::null(),&startup.StartupInfo,&mut info))?;}
    let process = Handle::new(info.hProcess)?; let thread = Handle::new(info.hThread)?;
    if unsafe {AssignProcessToJobObject(job.raw(),process.raw())} == 0 {unsafe {TerminateProcess(process.raw(),1);} anyhow::bail!("probe_job_assignment_failed");}
    if unsafe {ResumeThread(thread.raw())} == u32::MAX {anyhow::bail!("probe_resume_failed");}
    Ok(Probe {process,_job:job,_image:image})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    #[test]
    fn already_exited_engine_does_not_block_cleanup() {
        let mut child = std::process::Command::new("cmd.exe").args(["/d","/c","exit 0"]).creation_flags(CREATE_NO_WINDOW).spawn().unwrap();
        child.wait().unwrap(); terminate_process(child.as_raw_handle()).unwrap();
    }
    #[test]
    fn authenticated_parent_rejects_pid_birth_and_token_changes() {
        let mut owner=Process::open(std::process::id()).unwrap();owner.birth^=1;assert!(launch_parent(&owner).is_err());
        let mut owner=Process::open(std::process::id()).unwrap();owner.identity.authentication_id.0^=1;assert!(launch_parent(&owner).is_err());
        let mut owner=Process::open(std::process::id()).unwrap();owner.pid=0;assert!(launch_parent(&owner).is_err());
    }
    #[test]
    fn parent_duplicate_raii_closes_only_the_remote_copy_on_failure() {
        let owner=Process::open(std::process::id()).unwrap();let parent=Arc::new(launch_parent(&owner).unwrap());
        let event=Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())}).unwrap();
        let remote=ParentHandle::duplicate(&parent,event.raw()).unwrap();let remote_value=remote.raw;assert_ne!(remote_value,event.raw());assert_ne!(unsafe {CompareObjectHandles(remote_value,event.raw())},0);drop(remote);
        // Other tests may reuse the numeric slot; only the original object's identity matters.
        assert_eq!(unsafe {CompareObjectHandles(remote_value,event.raw())},0);
        assert_ne!(unsafe {SetEvent(event.raw())},0);assert_eq!(unsafe {WaitForSingleObject(event.raw(),0)},WAIT_OBJECT_0);
    }
    #[cfg(feature="privileged-validation")]
    #[test]
    fn diagnostic_watchdog_stderr_pipe_reaches_parent_without_stdout_or_network() {
        use std::io::Read;
        let helper=std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("termterm-vpn-broker.exe");
        let (read,write)=anonymous_pipe().unwrap();let lock=duplicate(unsafe {GetCurrentProcess()},false).unwrap();
        let error=watchdog_spawn(&helper,&lock,write.raw()).unwrap_err().to_string();assert!(error.contains("watchdog_not_ready"),"{error}");drop(write);
        let mut text=String::new();file(read).read_to_string(&mut text).unwrap();assert!(text.contains("elevated_watchdog_required"),"{text}");
    }
    use std::os::windows::process::CommandExt;
}
