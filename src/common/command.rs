use crate::protocol::IndexedResp;
use tokio::sync::oneshot;
use std::future::Future;
use std::task::{Context, Poll};
use std::pin::Pin;
use pin_project::pin_project;

pub fn reply_channel(req: IndexedResp) -> (ReplySender, ReplyReceiver) {
    let (s, r) = oneshot::channel();
    let sender = ReplySender {
        sender: s,
        req,
    };
    let receiver = ReplyReceiver { receiver: r };
    (sender, receiver)
}

type RespResult = Result<IndexedResp, anyhow::Error>;

pub struct ReplySender {
    sender: oneshot::Sender<RespResult>,
    req: IndexedResp,
}

impl ReplySender {
    pub fn set_result(self, result: RespResult) {
        self.sender.send(result).unwrap_or_default();
    }

    pub fn get_req(&self) -> &IndexedResp {
        &self.req
    }
}

#[pin_project]
pub struct ReplyReceiver {
    #[pin]
    receiver: oneshot::Receiver<RespResult>
}

impl Future for ReplyReceiver {
    type Output = RespResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let r = match std::task::ready!(self.project().receiver.poll(cx)) {
            Ok(r) => r,
            Err(err) => Err(err.into()),
        };
        Poll::Ready(r)
    }
}

