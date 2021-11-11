use pyo3::prelude::*;
use tokio::sync::oneshot::{Sender, channel};
use std::time::Duration;
use k8s_openapi::api::core::v1::Pod;
use pyo3::types::PyUnicode;
use reqwest::Url;
use futures::FutureExt;
use serde::Deserialize;
use futures_util::select;


#[pyclass]
struct OcfHeartbeat {
    signal: Option<Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Runtime>
}

#[pymethods]
impl OcfHeartbeat {
    #[new]
    fn new(dns: &PyUnicode, id: &PyUnicode,  ttl: u64) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let dns: Url = format!("{}/refresh", dns).parse().unwrap();
        let id = id.to_string();
        let (signal, recv) = channel();
        let recv = recv.fuse();
        let handle = runtime.spawn(async move {
            let mut client = reqwest::Client::default();
            let query = vec![("ticket".to_string(), id.clone())];
            futures::pin_mut!(recv);
            loop {
                let resp = client.post(dns.clone()).query(&query).send();
                let resp = resp.fuse();
                futures::pin_mut!(resp);
                let ticket: Response<KeepAliveTicket> = select! {
                    _ = recv => {
                        println!("HACKATHON received shutdown signal for {} while waiting on ticket lease", id);
                        return;
                    },
                    resp = resp => resp.unwrap().json().await.unwrap()
                };
                let until = chrono::NaiveDateTime::from_timestamp(ticket.payload.as_ref().unwrap().object.execution_date, 0);
                println!("HACKATHON leased a new ticket for {} from the ACM that is good until {:?}", id, until);
                let sleep = tokio::time::sleep(Duration::from_secs(ttl / 2));
                let sleep = sleep.fuse();
                futures::pin_mut!(sleep);
                select! {
                    _ = recv => {
                        println!("HACKATHON received shutdown signal for {} while thread was asleep", id);
                        return;
                    }
                    _ = sleep => ()
                };
            }
        });
        OcfHeartbeat {
            signal: Some(signal),
            handle: Some(handle),
            runtime: Some(runtime)
        }
    }
}

impl Drop for OcfHeartbeat {
    fn drop(&mut self) {
        let handle = std::mem::replace(&mut self.handle, None).unwrap();
        let signal = std::mem::replace(&mut self.signal, None).unwrap();
        let runtime = std::mem::replace(&mut self.runtime, None).unwrap();
        runtime.block_on(async move {
            let _ = signal.send(());
            handle.await;
        });
        runtime.shutdown_background();
        println!("HACKATHON successful async runtime shutdown");
    }
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn heartbeat(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<OcfHeartbeat>()?;
    Ok(())
}

#[derive(Deserialize, Debug, Clone)]
pub struct Response<T> {
    pub payload: Option<Payload<T>>,
    pub error: Option<Error>,
}

impl<T> Response<T> {
    pub fn expect(&self) -> &T {
        if let Some(payload) = self.payload.as_ref() {
            &payload.object
        } else {
            panic!("{:?}", self.error.as_ref().unwrap())
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Payload<T> {
    pub kind: String,
    pub object: T,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Error {
    pub kind: String,
    pub message: String,
    pub cause: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Image {
    pub tag: String,
    pub digest: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PodTicket {
    pub pod: Pod,
    pub ticket: Ticket,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Ticket {
    /// `ticket` is the unique identifier for this `KeepAliveTicket`
    ///
    /// It is currently simply the name of the pod that it is tied
    /// to, however this should not be taken to be a guarantee of
    /// future behavior.
    pub ticket: String,
    /// `execution_date` is the Unix timestamp of the exact moment
    /// when the `KeepAliveTicket` becomes invalid and deletion of
    /// the backing pod will commence.
    ///
    /// There is no grace period.
    pub execution_date: i64,
}

#[derive(Deserialize)]
pub struct KeepAliveTicket {
    /// `ticket` is the unique identifier for this `KeepAliveTicket`
    ///
    /// It is currently simply the name of the pod that it is tied
    /// to, however this should not be taken to be a guarantee of
    /// future behavior.
    ticket: String,
    /// `execution_date` is the Unix timestamp of the exact moment
    /// when the `KeepAliveTicket` becomes invalid and deletion of
    /// the backing pod will commence.
    ///
    /// There is no grace period.
    execution_date: i64,
}
