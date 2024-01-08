use std::{
    cell::RefCell,
    convert::TryInto,
    rc::Rc,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use aquatic_ws_protocol::{
    InMessage, InfoHash, OfferId, OutMessage, PeerId, RtcAnswer, RtcAnswerType,
};
use async_tungstenite::{client_async, WebSocketStream};
use futures::{SinkExt, StreamExt};
use futures_rustls::{client::TlsStream, TlsConnector};
use glommio::net::TcpStream;
use glommio::{prelude::*, timer::TimerActionRepeat};
use rand::{prelude::SmallRng, Rng, SeedableRng};

use crate::{common::LoadTestState, config::Config, utils::create_random_request};

pub async fn run_socket_thread(
    config: Config,
    tls_config: Arc<rustls::ClientConfig>,
    load_test_state: LoadTestState,
) -> anyhow::Result<()> {
    let config = Rc::new(config);
    let num_active_connections = Rc::new(RefCell::new(0usize));
    let connection_creation_interval =
        Duration::from_millis(config.connection_creation_interval_ms);

    TimerActionRepeat::repeat(move || {
        periodically_open_connections(
            config.clone(),
            tls_config.clone(),
            load_test_state.clone(),
            num_active_connections.clone(),
            connection_creation_interval,
        )
    })
    .join()
    .await
    .ok_or_else(|| anyhow::anyhow!("connection opener timer cancelled"))
}

async fn periodically_open_connections(
    config: Rc<Config>,
    tls_config: Arc<rustls::ClientConfig>,
    load_test_state: LoadTestState,
    num_active_connections: Rc<RefCell<usize>>,
    connection_creation_interval: Duration,
) -> Option<Duration> {
    if *num_active_connections.borrow() < config.num_connections_per_worker {
        spawn_local(async move {
            if let Err(err) =
                Connection::run(config, tls_config, load_test_state, num_active_connections).await
            {
                ::log::info!("connection creation error: {:#}", err);
            }
        })
        .detach();
    }

    Some(connection_creation_interval)
}

struct Connection {
    config: Rc<Config>,
    load_test_state: LoadTestState,
    rng: SmallRng,
    peer_id: PeerId,
    can_send_answer: Option<(InfoHash, PeerId, OfferId)>,
    stream: WebSocketStream<TlsStream<TcpStream>>,
}

impl Connection {
    async fn run(
        config: Rc<Config>,
        tls_config: Arc<rustls::ClientConfig>,
        load_test_state: LoadTestState,
        num_active_connections: Rc<RefCell<usize>>,
    ) -> anyhow::Result<()> {
        let mut rng = SmallRng::from_entropy();
        let peer_id = PeerId(rng.gen());
        let stream = TcpStream::connect(config.server_address)
            .await
            .map_err(|err| anyhow::anyhow!("connect: {:?}", err))?;
        let stream = TlsConnector::from(tls_config)
            .connect("example.com".try_into().unwrap(), stream)
            .await?;
        let request = format!(
            "ws://{}:{}",
            config.server_address.ip(),
            config.server_address.port()
        );
        let (stream, _) = client_async(request, stream).await?;

        let statistics = load_test_state.statistics.clone();

        let mut connection = Connection {
            config,
            load_test_state,
            rng,
            stream,
            peer_id,
            can_send_answer: None,
        };

        *num_active_connections.borrow_mut() += 1;
        statistics.connections.fetch_add(1, Ordering::Relaxed);

        if let Err(err) = connection.run_connection_loop().await {
            ::log::info!("connection error: {:#}", err);
        }

        *num_active_connections.borrow_mut() -= 1;
        statistics.connections.fetch_sub(1, Ordering::Relaxed);

        Ok(())
    }

    async fn run_connection_loop(&mut self) -> anyhow::Result<()> {
        loop {
            self.send_message().await?;
            self.read_message().await?;
        }
    }

    async fn send_message(&mut self) -> anyhow::Result<()> {
        let request = create_random_request(
            &self.config,
            &self.load_test_state,
            &mut self.rng,
            self.peer_id,
            self.can_send_answer.is_none(),
        );

        // If self.send_answer is set and request is announce request, make
        // the request an offer answer
        let request = if let InMessage::AnnounceRequest(mut r) = request {
            if let Some((info_hash, peer_id, offer_id)) = self.can_send_answer {
                r.info_hash = info_hash;
                r.answer_to_peer_id = Some(peer_id);
                r.answer_offer_id = Some(offer_id);
                r.answer = Some(RtcAnswer {
                    t: RtcAnswerType::Answer,
                    sdp: "abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-abcdefg-".into()
                });
                r.event = None;
                r.offers = None;
            }

            self.can_send_answer = None;

            InMessage::AnnounceRequest(r)
        } else {
            request
        };

        self.stream.send(request.to_ws_message()).await?;

        self.load_test_state
            .statistics
            .requests
            .fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    async fn read_message(&mut self) -> anyhow::Result<()> {
        let message = match self
            .stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("stream finished"))??
        {
            message @ tungstenite::Message::Text(_) | message @ tungstenite::Message::Binary(_) => {
                message
            }
            message => {
                ::log::warn!(
                    "Received WebSocket message of unexpected type: {:?}",
                    message
                );

                return Ok(());
            }
        };

        match OutMessage::from_ws_message(message) {
            Ok(OutMessage::OfferOutMessage(offer)) => {
                self.load_test_state
                    .statistics
                    .responses_offer
                    .fetch_add(1, Ordering::Relaxed);

                self.can_send_answer = Some((offer.info_hash, offer.peer_id, offer.offer_id));
            }
            Ok(OutMessage::AnswerOutMessage(_)) => {
                self.load_test_state
                    .statistics
                    .responses_answer
                    .fetch_add(1, Ordering::Relaxed);
            }
            Ok(OutMessage::AnnounceResponse(_)) => {
                self.load_test_state
                    .statistics
                    .responses_announce
                    .fetch_add(1, Ordering::Relaxed);
            }
            Ok(OutMessage::ScrapeResponse(_)) => {
                self.load_test_state
                    .statistics
                    .responses_scrape
                    .fetch_add(1, Ordering::Relaxed);
            }
            Ok(OutMessage::ErrorResponse(response)) => {
                self.load_test_state
                    .statistics
                    .responses_error
                    .fetch_add(1, Ordering::Relaxed);

                ::log::warn!("received error response: {:?}", response.failure_reason);
            }
            Err(err) => {
                ::log::error!("error deserializing message: {:#}", err);
            }
        }

        Ok(())
    }
}
