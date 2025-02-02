use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

#[derive(Default, Clone)]
struct State
{
    socket_running : bool,
    cancel_tx: Option<Arc<mpsc::Sender<()>>>,
}

#[derive(Debug, Clone)]
enum ButtonEvent
{
    StartSocket,
    StopSocket,
    SocketStopping(()),
    SocketStopped(())
}

#[tokio::main]
async fn main() -> iced::Result
{
    iced::run("Test", update, view)
}

fn update(state: &mut State, event: ButtonEvent) -> iced::Task<ButtonEvent>
{
    match event
    {
        ButtonEvent::StartSocket =>
        {
            state.socket_running = true;
            
            let (cancel_tx, cancel_rx) = mpsc::channel(1);
            state.cancel_tx = Some(Arc::new(cancel_tx));
            iced::Task::perform(kraken_socket(cancel_rx), ButtonEvent::SocketStopped)
        },
        ButtonEvent::StopSocket => 
        {
            if let Some(cancel_tx) = state.cancel_tx.clone()
            {
                iced::Task::perform(async move
                    {
                        let _ = cancel_tx.send(()).await;
                    },
                    ButtonEvent::SocketStopping
                )
            }
            else
            {
                println!("No sockets running");
                iced::Task::none()
            }
        },
        ButtonEvent::SocketStopping(_) =>
        {
            println!("Stopping socket...");
            iced::Task::none()
        },
        ButtonEvent::SocketStopped(_) =>
        {
            state.cancel_tx = None;
            state.socket_running = false;
            println!("Socket stopped");

            iced::Task::none()
        }
    }
}

fn view(state: &State) -> iced::widget::Row<ButtonEvent>
{
    if !state.socket_running
    {
        iced::widget::row!
        [
            iced::widget::button(iced::widget::text("Start")).on_press(ButtonEvent::StartSocket),
            iced::widget::button(iced::widget::text("Stop"))
        ]
    }
    else
    {
        iced::widget::row!
        [
            iced::widget::button(iced::widget::text("Start")),
            iced::widget::button(iced::widget::text("Stop")).on_press(ButtonEvent::StopSocket)
        ]
    }
}

async fn kraken_socket(mut cancel_rx: mpsc::Receiver<()>)
{
    let symbol = "BTC";
    let currency = "USD";
    let mut connected = false;
    let ws_url = "wss://ws.kraken.com/v2";

    let unix_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time travel to the past") - Duration::new(86400, 0);

    if let Ok((mut socket, _)) = connect_async(ws_url).await
    {
        println!("Successfully connected to Kraken");

        let req_json = serde_json::json!(
        {
            "method" : "subscribe",
            "params" : { "channel" : "ohlc", "symbol" : [symbol.to_owned() + &"/" + &currency], "interval" : 1 }
        });
        let message = Message::from(req_json.to_string());

        if let Err(e) = socket.send(message).await
        {
            eprintln!("Error sending message: {:?}", e);
        }

        tokio::select!
        {
            _ = cancel_rx.recv() =>
            {
                let _ = socket.close(None).await;
                return;
            }

            _ = async
            {
                while let Some(Ok(response)) = socket.next().await
                {
                    let resp_json : serde_json::Value = serde_json::from_str(response.to_text().unwrap()).unwrap();

                    if resp_json["success"] == true
                    {
                        connected = true;
                        continue;
                    }

                    if connected && resp_json["channel"] == "ohlc"
                    {
                        if resp_json["type"] == "snapshot"
                        {
                            for i in 0..resp_json["data"].as_array().unwrap().len()
                            {
                                println!("{}", resp_json["data"][i]["close"]);
                            }
                        }
                        else if resp_json["type"] == "update"
                        {
                            println!("{}", resp_json["data"][0]["close"]);
                        }
                    }
                }
            } => {}
        }
    }
    else
    {
        eprintln!("Failed to connect to Kraken");
    }
}
