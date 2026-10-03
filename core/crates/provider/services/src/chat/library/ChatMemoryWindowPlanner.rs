//! Direct port of Kotlin ChatMemoryWindowPlanner: bounded windows with user context
//! carried across a long assistant/group turn. Every source message appears once.
use crate::runtime_support::ProviderMemoryAutoSaveMessage;
#[derive(Clone, Debug)]
pub struct MemoryWindow {
    pub messages: Vec<ProviderMemoryAutoSaveMessage>,
    pub sourceMessageCount: usize,
}
pub fn planWindows(mut messages: Vec<ProviderMemoryAutoSaveMessage>, size: usize,
    fromInclusive: Option<i64>, toInclusive: Option<i64>) -> Vec<MemoryWindow> {
    let size = size.clamp(8,48);
    messages.retain(|m| fromInclusive.map(|t|m.timestamp>=t).unwrap_or(true)
        && toInclusive.map(|t|m.timestamp<=t).unwrap_or(true));
    messages.sort_by_key(|m|m.timestamp);
    let mut windows=Vec::new(); let mut source=Vec::new(); let mut context=Vec::new();
    let mut user:Option<ProviderMemoryAutoSaveMessage>=None;
    fn emit(windows:&mut Vec<MemoryWindow>,source:&mut Vec<ProviderMemoryAutoSaveMessage>,context:&mut Vec<ProviderMemoryAutoSaveMessage>) {
        if source.is_empty() { return; }
        let count=source.len();let mut messages=std::mem::take(context);messages.append(source);
        windows.push(MemoryWindow{messages,sourceMessageCount:count});
    }
    for message in messages {
        if message.content.trim().is_empty() {continue;}
        match message.sender.as_str() {
            "user" => {
                if source.len()>=size-1 { emit(&mut windows,&mut source,&mut context); }
                user=Some(message.clone());source.push(message);
            },
            "ai"|"assistant" => {
                let Some(user)=&user else {continue;};
                if source.len()>=size {emit(&mut windows,&mut source,&mut context);context.push(user.clone());}
                source.push(message);
            }, _=>{},
        }
    }
    emit(&mut windows,&mut source,&mut context);windows
}
#[cfg(test)]
mod tests {
    use super::*;
    fn message(t:i64,s:&str)->ProviderMemoryAutoSaveMessage { ProviderMemoryAutoSaveMessage{timestamp:t,sender:s.into(),content:format!("message {t}")} }
    #[test] fn every_group_reply_is_retained_with_user_context() {
        let mut input=vec![message(1,"user")]; input.extend((2..30).map(|t|message(t,"ai")));
        let windows=planWindows(input,8,None,None);
        assert_eq!(windows.iter().map(|w|w.sourceMessageCount).sum::<usize>(),29);
        assert!(windows.iter().all(|w|w.messages[0].sender=="user"));
        assert_eq!(windows.iter().flat_map(|w|&w.messages).filter(|m|m.sender=="ai").count(),28);
    }
    #[test] fn range_is_inclusive_and_ignores_orphan_replies() {
        let windows=planWindows(vec![message(1,"user"),message(2,"ai"),message(3,"user"),message(4,"ai")],32,Some(2),Some(4));
        assert_eq!(windows.len(),1);assert_eq!(windows[0].sourceMessageCount,2);assert_eq!(windows[0].messages[0].timestamp,3);
    }
}
