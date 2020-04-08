use std::net::SocketAddr;


#[derive(Clone)]
pub struct Config {
    pub num_threads: usize,
    pub network: NetworkConfig,
    pub statistics: StatisticsConfig,
    pub cleaning: CleaningConfig,
}


#[derive(Clone)]
pub struct NetworkConfig {
    pub address: SocketAddr,
    pub max_scrape_torrents: u8,
    pub max_response_peers: usize,
    pub peer_announce_interval: i32,
    pub recv_buffer_size: usize,
    pub poll_event_capacity: usize,
}


#[derive(Clone)]
pub struct StatisticsConfig {
    pub interval: u64,
}


#[derive(Clone)]
pub struct CleaningConfig {
    pub interval: u64,
    pub max_peer_age: u64,
    pub max_connection_age: u64,
}


impl Default for Config {
    fn default() -> Self {
        Self {
            num_threads: 4,
            network: NetworkConfig::default(),
            statistics: StatisticsConfig::default(),
            cleaning: CleaningConfig::default(),
        }
    }
}


impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            address: SocketAddr::from(([127, 0, 0, 1], 3000)),
            max_scrape_torrents: 255,
            max_response_peers: 255,
            peer_announce_interval: 60 * 15,
            poll_event_capacity: 4096,
            recv_buffer_size: 4096 * 128,
        }
    }
}


impl Default for StatisticsConfig {
    fn default() -> Self {
        Self {
            interval: 5,
        }
    }
}


impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            interval: 30,
            max_peer_age: 60 * 20,
            max_connection_age: 60 * 5,
        }
    }
}