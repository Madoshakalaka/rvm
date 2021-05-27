use std::ffi::{OsString};
use std::fmt::{Debug};
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};


use winapi::shared::minwindef::{BOOL, DWORD, LPCVOID};
use winapi::shared::ntdef::{HANDLE, LPWSTR};

use winapi::um::handleapi::CloseHandle;
use winapi::um::minwinbase::{LPSECURITY_ATTRIBUTES, SECURITY_ATTRIBUTES};
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::{CreateProcessW, GetStartupInfoW, PROCESS_INFORMATION, STARTUPINFOW};
use winapi::um::winbase::{CREATE_NEW_CONSOLE, STARTF_USESTDHANDLES};
use winapi::um::winnt::PHANDLE;
use winapi::um::winuser::WaitForInputIdle;


use shared::dep::tokio::io::AsyncWrite;


use winapi::um::fileapi::WriteFile;
use shared::dep::futures::task::{Context, Poll};
use shared::dep::futures_util::__private::Pin;
use shared::dep::futures::io::Error;


#[derive(Debug)]
struct ExtraTerminalWriter {
    pipe_entry: HANDLE,
    process: HANDLE,
    thread: HANDLE,
    child_stdin_read: HANDLE,
}

unsafe impl Send for ExtraTerminalWriter {}

pub struct AsyncExtraTerminalWriter {
    writer: Arc<Mutex<ExtraTerminalWriter>>,
}

impl AsyncWrite for AsyncExtraTerminalWriter {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>> {
        let mut bytes_written: u32 = 0;
        let ret = unsafe {
            WriteFile(self.writer.lock().unwrap().pipe_entry,
                      buf.as_ptr() as LPCVOID,
                      buf.len() as u32,
                      &mut bytes_written,
                      std::ptr::null_mut())
        };
        if ret == 0 {
            let error = Err(std::io::Error::last_os_error());
            return Poll::Ready(error);
        }
        Poll::Ready(Ok(bytes_written as usize))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let writer = self.writer.lock().unwrap();
        unsafe {
            CloseHandle(writer.pipe_entry);
            CloseHandle(writer.child_stdin_read);
            CloseHandle(writer.thread);
            CloseHandle(writer.process);
        }

        Poll::Ready(Ok(()))
    }
}

impl Default for AsyncExtraTerminalWriter {
    fn default() -> Self {
        Self { writer: Arc::new(Mutex::new(ExtraTerminalWriter::default())) }
    }
}


impl Default for ExtraTerminalWriter {
    /// the caller is responsible to close all four handles
    fn default() -> ExtraTerminalWriter {
        let exe_path: Vec<u16> = OsString::from(r#"C:\Users\Matt\CLionProjects\rvm\target\release\extra_log.exe"#)
            .encode_wide().chain(Some(0)).collect();
        let mut si: STARTUPINFOW = STARTUPINFOW::default();

        unsafe { GetStartupInfoW(&mut si as *mut _); }
        si.dwFlags |= STARTF_USESTDHANDLES;

        let mut pi: PROCESS_INFORMATION = PROCESS_INFORMATION::default();

        let mut child_stdin_read = std::ptr::null_mut();
        let mut child_stdin_write = std::ptr::null_mut();
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as DWORD,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true as BOOL,
        };

        unsafe {
            CreatePipe(&mut child_stdin_read as PHANDLE,
                       &mut child_stdin_write as PHANDLE,
                       &mut attributes as LPSECURITY_ATTRIBUTES, 0)
        };

        si.hStdInput = child_stdin_read;

        if unsafe {
            CreateProcessW(exe_path.as_ptr() as LPWSTR,
                           std::ptr::null_mut(),
                           std::ptr::null_mut(),
                           std::ptr::null_mut(),
                           i32::from(true),
                           CREATE_NEW_CONSOLE,
                           std::ptr::null_mut(),
                           std::ptr::null_mut(),
                           &mut si as *mut _,
                           &mut pi as *mut _)
        } == 0 {
            eprintln!("{:?}", std::io::Error::last_os_error());
            std::process::exit(-1);
        }


        unsafe { WaitForInputIdle(pi.hProcess, 5000); }

        let process = pi.hProcess.to_owned();
        let thread = pi.hThread.to_owned();

        ExtraTerminalWriter {
            pipe_entry: child_stdin_write,
            process,
            thread,
            child_stdin_read,
        }
    }
}









