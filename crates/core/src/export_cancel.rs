//! 导出原生会话的取消登记；弱引用不延长解码器/编码器生命周期。
use std::sync::{Arc, Mutex, Weak, atomic::AtomicBool};
pub type CancellationList = Arc<Mutex<Vec<Weak<AtomicBool>>>>;

/// 将本次原生会话的稳定取消句柄加入弱引用表，并移除已释放条目。
/// 监视器仅设置原子标志，不并发访问或释放任何 libav 上下文。
pub fn register(sessions: &CancellationList, cancel: &Arc<AtomicBool>) {
    let mut sessions = sessions.lock().unwrap();
    sessions.retain(|session| session.strong_count() > 0);
    sessions.push(Arc::downgrade(cancel));
}
