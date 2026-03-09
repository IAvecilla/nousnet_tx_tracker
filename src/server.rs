//! HTTP and WebSocket server for the transaction tracker.

use anyhow::Result;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{debug, error, info};

use crate::config::derive_coordinator_pda;
use crate::fetcher::{fetch_historical_transactions, HistoricalFetchConfig};
use crate::store::TransactionStore;
use crate::types::{
    parse_relative_time, FetchHistoryQuery, FetchHistoryResponse, TransactionInfo,
    TransactionQuery, TransactionStats, WsMessage,
};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<TransactionStore>,
    pub tx_broadcast: broadcast::Sender<TransactionInfo>,
    pub rpc_url: String,
}

/// Start the HTTP/WebSocket server
pub async fn start_server(
    port: u16,
    store: Arc<TransactionStore>,
    tx_broadcast: broadcast::Sender<TransactionInfo>,
    static_dir: Option<&str>,
    rpc_url: String,
) -> Result<()> {
    let state = AppState {
        store,
        tx_broadcast,
        rpc_url,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .route("/api/transactions", get(get_transactions))
        .route("/api/stats", get(get_stats))
        .route("/api/health", get(health_check))
        .route("/api/fetch-history", get(fetch_history_handler))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    // Serve static files if directory is provided
    if let Some(dir) = static_dir {
        app = app.nest_service("/", ServeDir::new(dir).append_index_html_on_directories(true));
    } else {
        // Serve embedded index page
        app = app.route("/", get(index_handler));
    }

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// GET /api/transactions - Query transactions
async fn get_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionQuery>,
) -> Json<Vec<TransactionInfo>> {
    Json(state.store.query(&query))
}

/// GET /api/stats - Get transaction statistics
async fn get_stats(
    State(state): State<AppState>,
    Query(params): Query<StatsQuery>,
) -> Json<TransactionStats> {
    Json(state.store.stats(params.run_id.as_deref()))
}

#[derive(serde::Deserialize)]
struct StatsQuery {
    run_id: Option<String>,
}

/// GET /api/health - Health check endpoint
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "psyche-tx-tracker",
        "transactions_in_memory": state.store.len()
    }))
}

/// GET /api/fetch-history - Fetch historical transactions for a run_id
async fn fetch_history_handler(
    State(state): State<AppState>,
    Query(query): Query<FetchHistoryQuery>,
) -> impl IntoResponse {
    // Validate run_id is present
    let run_id = match query.run_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            return Json(FetchHistoryResponse {
                fetched_count: 0,
                matched_count: 0,
                total_in_store: state.store.len(),
                complete: false,
                error: Some("run_id parameter is required".to_string()),
                transactions: vec![],
            });
        }
    };

    // Parse relative time, default to 1 day
    let since_str = query.since.as_deref().unwrap_or("1d");
    let seconds_ago = parse_relative_time(since_str).unwrap_or(86400); // Default 1 day

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let since_timestamp = now - seconds_ago;

    // Derive the coordinator PDA from the run_id
    let coordinator_pda = derive_coordinator_pda(&run_id).to_string();

    info!(
        "Fetching history for run_id={} (pda={}) since={} ({}s ago)",
        run_id, coordinator_pda, since_timestamp, seconds_ago
    );

    let config = HistoricalFetchConfig {
        run_id,
        coordinator_pda,
        since_timestamp,
        batch_size: 100,
        rate_limit_ms: 100,
    };

    let result = fetch_historical_transactions(
        &state.rpc_url,
        state.store.clone(),
        &state.tx_broadcast,
        config,
    )
    .await;

    Json(FetchHistoryResponse {
        fetched_count: result.fetched_count,
        matched_count: result.matched_count,
        total_in_store: result.total_in_store,
        complete: result.complete,
        error: result.error,
        transactions: result.transactions,
    })
}

/// GET / - Serve embedded index page
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

/// WebSocket handler for real-time updates
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rx = state.tx_broadcast.subscribe();
    ws.on_upgrade(move |socket| handle_ws(socket, rx))
}

/// Handle WebSocket connection
async fn handle_ws(socket: WebSocket, mut rx: broadcast::Receiver<TransactionInfo>) {
    let (mut sender, mut receiver) = socket.split();

    // Send connected message
    let connected_msg = WsMessage::Connected {
        message: "Connected to Psyche Transaction Tracker".to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    // Spawn task to forward broadcast messages to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(tx_info) = rx.recv().await {
            let ws_msg = WsMessage::NewTransaction(tx_info);
            match serde_json::to_string(&ws_msg) {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize WebSocket message: {}", e);
                }
            }
        }
    });

    // Handle incoming WebSocket messages (for ping/pong or future commands)
    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Ping(_)) => {
                    debug!("Received ping");
                }
                Ok(Message::Close(_)) => {
                    debug!("Client closed WebSocket connection");
                    break;
                }
                Ok(Message::Text(text)) => {
                    debug!("Received text message: {}", text);
                }
                Err(e) => {
                    error!("WebSocket receive error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    debug!("WebSocket connection closed");
}
