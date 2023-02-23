use crate::common::{reply_channel, ReplyReceiver, ReplySender};
use crate::protocol::{new_simple_packet_codec, IndexedResp, RespCodec};
use anyhow::{anyhow, Error, Result};
use bytes::Bytes;
use futures::{Sink, SinkExt, Stream, StreamExt, TryStreamExt};
use rand::prelude::SliceRandom;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::codec::Decoder;

type ConnSink<T> = Pin<Box<dyn Sink<T, Error = Error> + Send>>;
type ConnStream<T> = Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>;

struct Conn {
    reader: ConnStream<IndexedResp>,
    writer: ConnSink<Bytes>,
}

impl Conn {
    fn new(socket: TcpStream) -> Self {
        let (encoder, decoder) = new_simple_packet_codec::<Bytes, IndexedResp>();
        let (tx, rx) = RespCodec::new(encoder, decoder).framed(socket).split();
        let tx = tx.sink_map_err(Error::from);
        let rx = rx.map_err(Error::from);
        let writer = Box::pin(tx);
        let reader = Box::pin(rx);
        Self { writer, reader }
    }
}

pub async fn new_io_group(
    id: usize,
    backend_addresses: Vec<String>,
) -> Result<(IOGroupHandle, IOGroup)> {
    let mut backends = Vec::with_capacity(backend_addresses.len());
    for backend_address in backend_addresses {
        let sock = TcpStream::connect(&backend_address).await?;
        let backend = Backend::new(Conn::new(sock));
        backends.push(backend);
    }

    let (client_sender, client_receiver) = tokio::sync::mpsc::channel(128);
    let session_count = Arc::new(AtomicUsize::new(0));
    let handle = IOGroupHandle {
        id,
        client_sender,
        session_count: session_count.clone(),
    };
    let group = IOGroup {
        id,
        session_count,
        client_receiver,
        backends,
    };
    Ok((handle, group))
}

pub struct IOGroupHandle {
    id: usize,
    client_sender: Sender<TcpStream>,
    session_count: Arc<AtomicUsize>,
}

impl IOGroupHandle {
    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn get_session_count(&self) -> usize {
        self.session_count.load(Ordering::Relaxed)
    }

    pub async fn add_session(&self, session_conn: TcpStream) -> Result<(), TcpStream> {
        self.client_sender
            .send(session_conn)
            .await
            .map_err(|SendError(conn)| conn)
    }
}

pub struct IOGroup {
    id: usize,
    session_count: Arc<AtomicUsize>,
    client_receiver: Receiver<TcpStream>,
    backends: Vec<Backend>,
}

impl IOGroup {
    pub async fn run(self) -> anyhow::Result<()> {
        let IOGroup {
            id: group_id,
            session_count,
            mut client_receiver,
            mut backends,
        } = self;

        let session_id_gen = AtomicUsize::new(0);
        let mut sessions = HashMap::with_capacity(128);

        futures::future::poll_fn(|cx: &mut Context<'_>| -> Poll<()> {
            if let Some(conn) = Self::handle_new_session_conn(&mut client_receiver, cx) {
                let session_id = session_id_gen.fetch_add(1, Ordering::Relaxed);
                let session = Session::new(session_id, conn);
                sessions.insert(session_id, session);
                session_count.fetch_add(1, Ordering::Relaxed);
            }

            let mut err_sessions = Vec::default();
            for (id, session) in sessions.iter_mut() {
                if session.handle(cx, &mut backends).is_err() {
                    err_sessions.push(*id);
                }
            }
            for id in err_sessions.into_iter() {
                tracing::warn!("closing failed sessions {} group: {}", id, group_id);
                sessions.remove(&id);
                session_count.fetch_sub(1, Ordering::Relaxed);
            }

            for backend in backends.iter_mut() {
                if backend.handle(cx).is_err() {
                    return Poll::Ready(());
                }
            }

            Poll::Pending
        })
        .await;

        Ok(())
    }

    fn handle_new_session_conn(
        client_receiver: &mut Receiver<TcpStream>,
        cx: &mut Context<'_>,
    ) -> Option<Conn> {
        match Pin::new(client_receiver).poll_recv(cx) {
            Poll::Ready(Some(conn)) => Some(Conn::new(conn)),
            _ => None,
        }
    }
}

struct Backend {
    conn: Conn,
    reqs: VecDeque<ReplySender>,
    packets: VecDeque<Bytes>,
}

impl Backend {
    fn new(conn: Conn) -> Self {
        Self {
            conn,
            reqs: VecDeque::with_capacity(4096),
            packets: VecDeque::with_capacity(4096),
        }
    }

    fn send(&mut self, reply_sender: ReplySender) {
        self.packets.push_back(reply_sender.get_req().get_bytes());
        self.reqs.push_back(reply_sender);
    }

    fn handle(&mut self, cx: &mut Context<'_>) -> anyhow::Result<()> {
        let writer = &mut self.conn.writer;
        let reader = &mut self.conn.reader;
        let reqs = &mut self.reqs;
        let packets = &mut self.packets;

        if let Err(err) = Self::handle_write(writer, cx, packets) {
            tracing::error!("failed to write packet to backend: {}", err);
            Self::set_reqs_err(reqs);
            return Err(err);
        }

        if let Err(err) = Self::handle_read(reader, cx, reqs) {
            tracing::error!("failed to read packet from backend: {}", err);
            Self::set_reqs_err(reqs);
            return Err(err);
        }

        Ok(())
    }

    fn set_reqs_err(reqs: &mut VecDeque<ReplySender>) {
        for req in reqs.drain(..) {
            req.set_result(Err(anyhow!("backend connection error")));
        }
    }

    fn handle_write(
        writer: &mut ConnSink<Bytes>,
        cx: &mut Context<'_>,
        packets: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<()> {
        loop {
            match writer.as_mut().poll_ready(cx) {
                Poll::Pending => return Ok(()),
                Poll::Ready(res) => res?,
            }

            match packets.pop_front() {
                Some(pkt) => writer.as_mut().start_send(pkt)?,
                None => {
                    return match writer.as_mut().poll_flush(cx) {
                        Poll::Pending => Ok(()),
                        Poll::Ready(res) => res,
                    }
                }
            }
        }
    }

    fn handle_read(
        reader: &mut ConnStream<IndexedResp>,
        cx: &mut Context<'_>,
        reqs: &mut VecDeque<ReplySender>,
    ) -> anyhow::Result<()> {
        loop {
            let packet_res = match reader.as_mut().poll_next(cx) {
                Poll::Ready(None) => {
                    tracing::error!("backend closed by peer");
                    return Err(anyhow!("backend closed"));
                }
                Poll::Ready(Some(r)) => r,
                Poll::Pending => return Ok(()),
            };

            let req = reqs
                .pop_front()
                .ok_or_else(|| anyhow!("INVALID req not found"))?;
            match packet_res {
                Ok(pkt) => req.set_result(Ok(pkt)),
                Err(err) => {
                    tracing::error!("failed to get response from backend: {}", err);
                    req.set_result(Err(anyhow!("backend error")));
                    return Err(err);
                }
            }
        }
    }
}

struct Session {
    id: usize,
    conn: Conn,

    pending_replies: VecDeque<ReplyReceiver>,
    packets: VecDeque<IndexedResp>,
}

impl Session {
    fn new(id: usize, conn: Conn) -> Self {
        Self {
            id,
            conn,
            pending_replies: VecDeque::with_capacity(4096),
            packets: VecDeque::with_capacity(4096),
        }
    }

    fn handle(&mut self, cx: &mut Context<'_>, backends: &mut [Backend]) -> anyhow::Result<()> {
        let writer = &mut self.conn.writer;
        let reader = &mut self.conn.reader;
        let pending_replies = &mut self.pending_replies;
        let packets = &mut self.packets;

        if let Err(err) = Self::handle_read(reader, cx, backends, pending_replies) {
            tracing::warn!("failed to handle session read: {} {}", err, self.id);
            return Err(err);
        }

        while let Some(reply_receiver) = pending_replies.front_mut() {
            let reply_res = match Pin::new(reply_receiver).poll(cx) {
                Poll::Pending => break,
                Poll::Ready(r) => r,
            };
            pending_replies.pop_front();

            match reply_res {
                Ok(reply) => packets.push_back(reply),
                Err(err) => {
                    tracing::error!("failed to get command response: {} {}", err, self.id);
                    return Err(err);
                }
            }
        }

        if let Err(err) = Self::handle_write(writer, cx, packets) {
            tracing::error!("failed to handle write in session: {} {}", err, self.id);
            return Err(err);
        }

        Ok(())
    }

    fn handle_read(
        reader: &mut ConnStream<IndexedResp>,
        cx: &mut Context<'_>,
        backends: &mut [Backend],
        pending_replies: &mut VecDeque<ReplyReceiver>,
    ) -> anyhow::Result<()> {
        let mut rng = rand::thread_rng();

        while let Poll::Ready(item) = reader.as_mut().poll_next(cx) {
            let pkt = match item {
                None => return Err(anyhow!("session connection closed by peer")),
                Some(res) => res?,
            };
            let (s, r) = reply_channel(pkt);
            // Random send
            let backend = backends
                .choose_mut(&mut rng)
                .ok_or_else(|| anyhow!("no backend found"))?;
            backend.send(s);
            pending_replies.push_back(r);
        }
        Ok(())
    }

    fn handle_write(
        writer: &mut ConnSink<Bytes>,
        cx: &mut Context<'_>,
        packets: &mut VecDeque<IndexedResp>,
    ) -> anyhow::Result<()> {
        loop {
            match writer.as_mut().poll_ready(cx) {
                Poll::Pending => return Ok(()),
                Poll::Ready(res) => res?,
            }

            let packet = match packets.pop_front() {
                Some(pkt) => pkt,
                None => {
                    return match writer.as_mut().poll_flush(cx) {
                        Poll::Pending => Ok(()),
                        Poll::Ready(res) => res,
                    };
                }
            };
            writer.as_mut().start_send(packet.get_bytes())?;
        }
    }
}
