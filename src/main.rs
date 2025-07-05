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
    price: String,
}

#[derive(Debug, Clone)]
enum Event
{
    StartSocket,
    StopSocket,
    SocketStopping(()),
    SocketStopped(())
}

#[tokio::main]
async fn main() -> iced::Result
{
    iced::run("OpenCharting", update, view)
}

fn update(state: &mut State, event: Event) -> iced::Task<Event>
{
    match event
    {
        Event::StartSocket =>
        {
            state.socket_running = true;
            println!("Starting socket...");
            
            let (cancel_tx, cancel_rx) = mpsc::channel(1);
            state.cancel_tx = Some(Arc::new(cancel_tx));
            iced::Task::perform(kraken_socket(cancel_rx), Event::SocketStopped)
        },
        Event::StopSocket => 
        {
            if let Some(cancel_tx) = state.cancel_tx.clone()
            {
                iced::Task::perform(async move
                    {
                        let _ = cancel_tx.send(()).await;
                    },
                    Event::SocketStopping
                )
            }
            else
            {
                println!("No sockets running");
                iced::Task::none()
            }
        },
        Event::SocketStopping(_) =>
        {
            println!("Stopping socket...");
            iced::Task::none()
        },
        Event::SocketStopped(_) =>
        {
            state.cancel_tx = None;
            state.socket_running = false;
            println!("Socket stopped");

            iced::Task::none()
        }
    }
}

fn view(state: &State) -> iced::Element<'_, Event>
{
    let title_text = iced::widget::text("OpenCharting").size(28);
    let symbol_text = iced::widget::text("Kraken BTC/USD").size(40);
    let symbol_value = iced::widget::text("$".to_owned() + &state.price).size(32);

    let button_row = if !state.socket_running
    {
        iced::widget::row!
        [
            iced::widget::button(iced::widget::text("Start")).on_press(Event::StartSocket),
            iced::widget::button(iced::widget::text("Stop"))
        ]
    }
    else
    {
        iced::widget::row!
        [
            iced::widget::button(iced::widget::text("Start")),
            iced::widget::button(iced::widget::text("Stop")).on_press(Event::StopSocket)
        ]
    }.spacing(20);

    let center_column = iced::widget::column!
    [
        symbol_text,
        symbol_value,
        button_row
    ];

    let center_container = iced::widget::Container::new(center_column).center(iced::Length::Fill);

    iced::widget::column!
    [
        title_text,
        center_container,
    ].into()
}

async fn kraken_socket(mut cancel_rx: mpsc::Receiver<()>)
{
    let symbol = "BTC";
    let currency = "USD";
    let mut connected = false;
    let ws_url = "wss://ws.kraken.com/v2";

    let _unix_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time travel to the past") - Duration::new(86400, 0);

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
