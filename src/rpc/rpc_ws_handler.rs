// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::rpc_api::data_types::JsonRpcServerState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::IntoResponse,
};
use crossbeam::atomic::AtomicCell;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use http::{HeaderMap, HeaderValue};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::rpc::rpc_util::{
    call_rpc_str, check_permissions, get_auth_header, get_error_str, is_v1_method,
};

async fn rpc_ws_task(
    authorization_header: Option<HeaderValue>,
    rpc_call: jsonrpc_v2::RequestObject,
    rpc_server: JsonRpcServerState,
    _is_socket_active: Arc<AtomicCell<bool>>,
    ws_sender: Arc<RwLock<SplitSink<WebSocket, Message>>>,
) -> anyhow::Result<()> {
    let call_method = rpc_call.method_ref();
    let _call_id = rpc_call.id_ref();

    check_permissions(rpc_server.clone(), call_method, authorization_header)
        .await
        .map_err(|(_, e)| anyhow::Error::msg(e))?;

    debug!("RPC WS called method: {}", call_method);
    let response = call_rpc_str(rpc_server.clone(), rpc_call).await?;
    ws_sender
        .write()
        .await
        .send(Message::Text(response))
        .await?;

    Ok(())
}

// Lotus exposes two versions of its RPC API: v0 and v1. Version 0 is almost a
// subset of version 1 (some methods such as `BeaconGetEntry` are only in v0 and
// not in v1). Forest deviates from Lotus in this regard and our v1 API is
// strictly a superset of the v0 API.
//
// This WS handler rejects RPC calls if they're not v0 methods.
pub async fn rpc_v0_ws_handler(
    headers: HeaderMap,
    axum::extract::State(rpc_server): axum::extract::State<JsonRpcServerState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authorization_header = get_auth_header(headers);
    ws.on_upgrade(move |socket| async {
        rpc_ws_handler_inner(socket, authorization_header, rpc_server, true).await
    })
}

pub async fn rpc_ws_handler(
    headers: HeaderMap,
    axum::extract::State(rpc_server): axum::extract::State<JsonRpcServerState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authorization_header = get_auth_header(headers);
    ws.on_upgrade(move |socket| async {
        rpc_ws_handler_inner(socket, authorization_header, rpc_server, false).await
    })
}

async fn rpc_ws_handler_inner(
    socket: WebSocket,
    authorization_header: Option<HeaderValue>,
    rpc_server: JsonRpcServerState,
    reject_v1_methods: bool,
) {
    debug!("Accepted WS connection!");
    let (sender, mut receiver) = socket.split();
    let ws_sender = Arc::new(RwLock::new(sender));
    let socket_active = Arc::new(AtomicCell::new(true));
    while let Some(Ok(message)) = receiver.next().await {
        debug!("Received new WS RPC message: {:?}", message);

        let payload: Option<Result<jsonrpc_v2::RequestObject, serde_json::Error>> = match message {
            Message::Text(request_text) => {
                if !request_text.is_empty() {
                    Some(serde_json::from_str(&request_text))
                } else {
                    None
                }
            }
            Message::Binary(request_data) => {
                if !request_data.is_empty() {
                    Some(serde_json::from_slice(&request_data))
                } else {
                    None
                }
            }
            // We should not need to support other kind of messages.
            _ => None,
        };

        if let Some(request_obj) = payload {
            debug!("RPC Request Received: {:?}", &request_obj);
            let authorization_header = authorization_header.clone();
            let task_rpc_server = rpc_server.clone();
            let task_socket_active = socket_active.clone();
            let task_ws_sender = ws_sender.clone();
            match request_obj {
                Ok(rpc_call) => {
                    if reject_v1_methods && is_v1_method(rpc_call.method_ref()) {
                        let msg = "This endpoint cannot handle v1 (unstable) methods".into();
                        error!("{}", msg);
                        return task_ws_sender
                            .write()
                            .await
                            .send(Message::Text(get_error_str(3, msg)))
                            .await
                            .unwrap();
                    }
                    tokio::task::spawn(async move {
                        match rpc_ws_task(
                            authorization_header,
                            rpc_call,
                            task_rpc_server,
                            task_socket_active,
                            task_ws_sender.clone(),
                        )
                        .await
                        {
                            Ok(_) => {
                                debug!("WS RPC task success.");
                            }
                            Err(e) => {
                                let msg = format!("WS RPC task error: {e}");
                                error!("{}", msg);
                                task_ws_sender
                                    .write()
                                    .await
                                    .send(Message::Text(get_error_str(3, msg)))
                                    .await
                                    .unwrap();
                            }
                        }
                    });
                }
                Err(e) => {
                    let msg = format!("Error deserializing WS request payload: {e}");
                    error!("{}", msg);
                    if let Err(e) = task_ws_sender
                        .write()
                        .await
                        .send(Message::Text(get_error_str(1, msg)))
                        .await
                    {
                        warn!("{e}");
                    }
                }
            }
        }
    }
    socket_active.store(false);
}
