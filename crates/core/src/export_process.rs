//! 导出子进程生命周期：排空错误管道，取消时终止，所有出口回收进程。
use std::{
    io::Read,
    process::{Child, Command},
    sync::{Arc, Mutex, Weak},
    thread::JoinHandle,
};

pub type ProcessList = Arc<Mutex<Vec<Weak<Mutex<Child>>>>>;

pub struct Process {
    pub child: Arc<Mutex<Child>>,
    errors: Option<JoinHandle<String>>,
}

impl Process {
    /// 启动受控命令并登记取消句柄，后台持续排空 stderr，仅保留最后 64KiB 诊断。
    /// 启动失败返回实际系统错误；调用方应在进程存活期间消费 stdout 或 stdin。
    pub fn spawn(command: &mut Command, processes: &ProcessList) -> Result<Self, String> {
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动导出媒体进程：{e}"))?;
        let errors = child.stderr.take().map(|mut stderr| {
            std::thread::spawn(move || {
                let mut output = Vec::new();
                let mut bytes = [0; 4096];
                while let Ok(count) = stderr.read(&mut bytes) {
                    if count == 0 {
                        break;
                    }
                    output.extend_from_slice(&bytes[..count]);
                    if output.len() > 65536 {
                        output.drain(..output.len() - 65536);
                    }
                }
                String::from_utf8_lossy(&output).trim().to_owned()
            })
        });
        let child = Arc::new(Mutex::new(child));
        register(processes, &child);
        Ok(Self { child, errors })
    }

    /// 在管道消费完毕后轮询等待子进程，不持锁阻塞取消监视线程；失败带真实 stderr。
    /// 成功也回收诊断线程，避免 EOF 与错误发布的竞态。
    pub fn finish(&mut self) -> Result<(), String> {
        let status = loop {
            if let Some(status) = self
                .child
                .lock()
                .unwrap()
                .try_wait()
                .map_err(|e| format!("读取导出进程状态失败：{e}"))?
            {
                break status;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let message = self
            .errors
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        if status.success() {
            Ok(())
        } else if message.is_empty() {
            Err(format!("导出媒体进程失败：{status}"))
        } else {
            Err(message)
        }
    }
}

impl Drop for Process {
    /// 无论正常、取消或错误退出，终止未完成进程并回收 stderr 线程；不删除源文件。
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        if let Some(thread) = self.errors.take() {
            let _ = thread.join();
        }
    }
}

/// 将解码/编码进程加入本次导出的弱引用表，清理已释放条目，不延长子进程生命周期。
pub fn register(processes: &ProcessList, child: &Arc<Mutex<Child>>) {
    let mut processes = processes.lock().unwrap();
    processes.retain(|process| process.strong_count() > 0);
    processes.push(Arc::downgrade(child));
}
