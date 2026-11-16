use std::io;

use p2p_app::{Config, Event, Node, netlib};
use tokio::io::{AsyncBufReadExt, BufReader};

const HELP: &str = "commands: /list | /connect <index> | /disconnect | /quit | other text is sent";

#[tokio::main]
async fn main() -> io::Result<()> {
    let cfg = Config { tcp_port: 47801, discovery_port: 47800 };
    let node = netlib::start(cfg, print_event).await?;
    println!("node {} on tcp {}\n{HELP}", node.id().short(), node.tcp_port());

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "" => {}
            "/list" => list(&node).await,
            "/connect" => connect(&node, rest).await,
            "/disconnect" => netlib::disconnect(&node),
            "/quit" => break,
            _ if cmd.starts_with('/') => println!("{HELP}"),
            _ => {
                match netlib::send(&node, line) {
                    Ok(()) => println!("{} (you): {line}", node.id().short()),
                    Err(e) => println!("not sent: {e}"),
                }
            }
        }
    }
    Ok(())
}

async fn list(node: &Node) {
    match netlib::list(node).await {
        Ok(peers) if peers.is_empty() => println!("nobody found"),
        Ok(peers) => {
            for (i, (id, addr)) in peers.iter().enumerate() {
                println!("  [{i}] {} {addr}", id.short());
            }
        }
        Err(e) => println!("list failed: {e}"),
    }
}

async fn connect(node: &Node, arg: &str) {
    let Ok(index) = arg.parse() else {
        return println!("usage: /connect <index>");
    };
    if let Err(e) = netlib::connect(node, index).await {
        println!("connect failed: {e}");
    }
}

fn print_event(ev: Event) {
    match ev {
        Event::Connected(id, addr) => println!("* connected {} at {addr}", id.short()),
        Event::Disconnected(id) => println!("* disconnected {}", id.short()),
        Event::Message { from, data } => {
            println!("{}: {}", from.short(), String::from_utf8_lossy(&data))
        }
    }
}
