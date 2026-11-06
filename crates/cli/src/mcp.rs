use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Map, Value as Json, json};

use crate::commands::{
    Item, Kind, Level, PlaybackCommand, ProviderCommand, QueueCommand, Repeat, Request,
    SearchQuery, Switch, Time,
};
use crate::item;
use crate::run::{describe, json as printed};
use crate::wire::{self, Reply};

/// The MCP versions this server speaks, newest first. A client asking for another is offered
/// the newest, as the specification asks.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// How many hits `search` answers when the caller names no limit.
const HITS: u16 = 10;
/// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const NO_METHOD: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What a tool call is answered with when the exchange with Sonora breaks before a reply.
type Asked = Result<Reply, wire::Error>;

/// Speaks MCP over `input` and `output`, one JSON-RPC message per line, until the client
/// closes `input`. Every tool call opens its own connection with `connect`, as a command line
/// does, so Sonora may start or quit while the client stays attached.
pub fn serve<S: io::Read + io::Write>(
    input: impl BufRead,
    mut output: impl Write,
    mut connect: impl FnMut() -> io::Result<S>,
) -> ExitCode {
    let mut ask = |request: &Request| {
        let mut stream = connect().map_err(wire::Error::Io)?;
        wire::exchange(&mut stream, request)
    };
    for line in input.lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some(answer) = respond(&line, &mut ask) else {
            continue;
        };
        if writeln!(output, "{answer}")
            .and_then(|()| output.flush())
            .is_err()
        {
            break;
        }
    }
    ExitCode::SUCCESS
}

/// Answers one line from the client: the reply to send back, or `None` for a notification.
fn respond(line: &str, ask: &mut impl FnMut(&Request) -> Asked) -> Option<Json> {
    let message: Json = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(failure(Json::Null, PARSE_ERROR, &error.to_string())),
    };
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Json::as_str) else {
        // An answer to a request this server never sends, or no message at all.
        return id.map(|id| failure(id, INVALID_REQUEST, "not a request"));
    };
    let id = id?;
    let params = message.get("params").cloned().unwrap_or(Json::Null);
    let result = match method {
        "initialize" => initialize(&params),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => match call(&params, ask) {
            Ok(result) => result,
            Err(message) => return Some(failure(id, INVALID_PARAMS, &message)),
        },
        _ => return Some(failure(id, NO_METHOD, &format!("no method '{method}'"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// The handshake: the protocol both ends speak, and that this server offers tools.
fn initialize(params: &Json) -> Json {
    let asked = params.get("protocolVersion").and_then(Json::as_str);
    let protocol = PROTOCOLS
        .into_iter()
        .find(|known| Some(*known) == asked)
        .unwrap_or(PROTOCOLS[0]);
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "sonora", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Controls the Sonora music player running on this computer. \
            Search and queue act on the current provider; switch_provider changes it.",
    })
}

/// Carries out a `tools/call`. An unknown tool is an error of the call itself; anything that
/// goes wrong past that is a tool result marked as an error, so the model can read why.
fn call(params: &Json, ask: &mut impl FnMut(&Request) -> Asked) -> Result<Json, String> {
    let name = params
        .get("name")
        .and_then(Json::as_str)
        .ok_or("a tool call names no tool")?;
    let empty = Map::new();
    let arguments = params
        .get("arguments")
        .and_then(Json::as_object)
        .unwrap_or(&empty);
    let Some(request) = request(name, arguments) else {
        return Err(format!("no tool '{name}'"));
    };
    let answer = request.and_then(|request| match ask(&request) {
        Ok(Reply::Done) => Ok("done".to_owned()),
        Ok(Reply::Value(value)) => Ok(printed(&value)),
        Ok(Reply::Failed(failure)) => Err(describe(&failure)),
        Err(wire::Error::Io(error)) if absent(&error) => {
            Err("Sonora is not running; start it first".to_owned())
        }
        Err(wire::Error::Closed) => {
            Err("the running Sonora did not answer; it may be older, restart it".to_owned())
        }
        Err(error) => Err(error.to_string()),
    });
    let (text, error) = match answer {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": error }))
}

/// The request a tool stands for, `None` for a tool that does not exist, or why its arguments
/// make no request.
fn request(name: &str, arguments: &Map<String, Json>) -> Option<Result<Request, String>> {
    let play = |command| Ok(Request::Playback(command));
    Some(match name {
        "now_playing" => play(PlaybackCommand::CurrentTrack),
        "playback" => match text(arguments, "action") {
            Some("play") => play(PlaybackCommand::Play),
            Some("pause") => play(PlaybackCommand::Pause),
            Some("toggle") => play(PlaybackCommand::Toggle),
            Some("next") => play(PlaybackCommand::Next),
            Some("previous") => play(PlaybackCommand::Previous),
            _ => Err("action is play, pause, toggle, next or previous".to_owned()),
        },
        "seek" => match (number(arguments, "to"), number(arguments, "by")) {
            (Some(to), None) if to >= 0 => play(PlaybackCommand::SeekTime(Time::To(to as u64))),
            (None, Some(by)) => play(PlaybackCommand::SeekTime(Time::By(by))),
            _ => Err("give either to, in seconds from the start, or by, a signed step".to_owned()),
        },
        "volume" => match (number(arguments, "level"), number(arguments, "by")) {
            (None, None) => play(PlaybackCommand::Volume(None)),
            (Some(level), None) => play(PlaybackCommand::Volume(Some(Level::To(
                level.clamp(0, 100) as u8,
            )))),
            (None, Some(by)) => play(PlaybackCommand::Volume(Some(Level::By(
                by.clamp(-100, 100) as i8,
            )))),
            _ => Err("give level or by, not both".to_owned()),
        },
        "shuffle" => match text(arguments, "mode") {
            None => play(PlaybackCommand::Shuffle(None)),
            Some("on") => play(PlaybackCommand::Shuffle(Some(Switch::On))),
            Some("off") => play(PlaybackCommand::Shuffle(Some(Switch::Off))),
            Some("toggle") => play(PlaybackCommand::Shuffle(Some(Switch::Toggle))),
            Some(_) => Err("mode is on, off or toggle".to_owned()),
        },
        "repeat" => match text(arguments, "mode") {
            None => play(PlaybackCommand::Repeat(None)),
            Some("off") => play(PlaybackCommand::Repeat(Some(Repeat::Off))),
            Some("all") => play(PlaybackCommand::Repeat(Some(Repeat::All))),
            Some("one") => play(PlaybackCommand::Repeat(Some(Repeat::One))),
            Some(_) => Err("mode is off, all or one".to_owned()),
        },
        "search" => search(arguments),
        "queue" => queue(arguments),
        "providers" => Ok(Request::Provider(ProviderCommand::List)),
        "switch_provider" => match text(arguments, "slug") {
            Some(slug) => Ok(Request::Provider(ProviderCommand::Current(Some(
                slug.to_owned(),
            )))),
            None => Err("slug names the provider, as providers lists it".to_owned()),
        },
        _ => return None,
    })
}

fn search(arguments: &Map<String, Json>) -> Result<Request, String> {
    let query = text(arguments, "query").ok_or("query is the words to search for")?;
    let kind = match text(arguments, "type") {
        None => None,
        Some(kind) => Some(kind_of(kind)?),
    };
    let limit = number(arguments, "limit").map_or(HITS, |limit| limit.clamp(1, 50) as u16);
    Ok(Request::Search(SearchQuery {
        text: query.to_owned(),
        kind,
        limit,
    }))
}

fn queue(arguments: &Map<String, Json>) -> Result<Request, String> {
    let items = match (text(arguments, "search"), arguments.get("items")) {
        (Some(query), None) => vec![Item::Search {
            text: query.to_owned(),
            kind: text(arguments, "type").map_or(Ok(Kind::Song), kind_of)?,
        }],
        (None, Some(Json::Array(items))) if !items.is_empty() => items
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .ok_or("items are strings".to_owned())
                    .and_then(queued)
            })
            .collect::<Result<_, _>>()?,
        _ => return Err("give items, links or absolute paths, or search, not both".to_owned()),
    };
    match text(arguments, "when") {
        None | Some("last") => Ok(Request::Queue(QueueCommand::Add(items))),
        Some("next") => Ok(Request::Queue(QueueCommand::PlayNext(items))),
        Some("now") => Ok(Request::Queue(QueueCommand::Play(items))),
        Some(_) => Err("when is now, next or last".to_owned()),
    }
}

/// A link, or a file the running Sonora can open. A relative path is refused, since this
/// process does not share the folder the model has in mind.
fn queued(entry: &str) -> Result<Item, String> {
    if item::is_address(entry) {
        return Ok(Item::Link(entry.to_owned()));
    }
    let path = item::local_path(entry);
    match (path.is_absolute(), path.exists()) {
        (false, _) => Err(format!("'{entry}' is not an absolute path or a link")),
        (true, false) => Err(format!("no file at '{}'", Path::new(&path).display())),
        (true, true) => Ok(Item::Path(path)),
    }
}

fn kind_of(kind: &str) -> Result<Kind, String> {
    match kind {
        "song" => Ok(Kind::Song),
        "album" => Ok(Kind::Album),
        "artist" => Ok(Kind::Artist),
        "playlist" => Ok(Kind::Playlist),
        _ => Err("type is song, album, artist or playlist".to_owned()),
    }
}

fn text<'a>(arguments: &'a Map<String, Json>, key: &str) -> Option<&'a str> {
    arguments.get(key).and_then(Json::as_str)
}

fn number(arguments: &Map<String, Json>, key: &str) -> Option<i64> {
    arguments.get(key).and_then(Json::as_i64)
}

/// Whether a failed connection means no Sonora listens, rather than one that broke.
fn absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

fn failure(id: Json, code: i64, message: &str) -> Json {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Every tool, as `tools/list` offers it.
fn tools() -> Json {
    let none = json!({ "type": "object", "properties": {} });
    let kinds = json!(["song", "album", "artist", "playlist"]);
    json!([
        {
            "name": "now_playing",
            "description": "What Sonora plays now: state, track, position and volume in seconds \
                and percent, shuffle, repeat, provider and the collection it plays from. \
                null while nothing is loaded.",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "playback",
            "description": "Play, pause, toggle, or skip to the next or previous track.",
            "inputSchema": {
                "type": "object",
                "properties": { "action": {
                    "type": "string",
                    "enum": ["play", "pause", "toggle", "next", "previous"],
                } },
                "required": ["action"],
            },
        },
        {
            "name": "seek",
            "description": "Move within the current track: to a second from its start, or by a \
                signed step in seconds. Held inside the track.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "to": { "type": "integer", "minimum": 0, "description": "Seconds from the start." },
                    "by": { "type": "integer", "description": "Seconds forward, or back when negative." },
                },
            },
        },
        {
            "name": "volume",
            "description": "Read the volume as a percent, or set it with level or step it with by.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "level": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "by": { "type": "integer", "minimum": -100, "maximum": 100 },
                },
            },
        },
        {
            "name": "shuffle",
            "description": "Read shuffle, or switch it.",
            "inputSchema": {
                "type": "object",
                "properties": { "mode": { "type": "string", "enum": ["on", "off", "toggle"] } },
            },
        },
        {
            "name": "repeat",
            "description": "Read repeat, or set it.",
            "inputSchema": {
                "type": "object",
                "properties": { "mode": { "type": "string", "enum": ["off", "all", "one"] } },
            },
        },
        {
            "name": "search",
            "description": "Search the current provider. Each hit carries the link queue takes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "type": { "type": "string", "enum": kinds },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": HITS },
                },
                "required": ["query"],
            },
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "queue",
            "description": "Queue links of the current provider or absolute paths of local \
                files, or the best hit of a search. when: now plays them at once with the \
                queue carrying on after, next puts them after the current track, last \
                (the default) after the tracks queued by hand. Albums, playlists and artists \
                queue their tracks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "search": { "type": "string", "description": "Queue the best hit for these words instead of items." },
                    "type": { "type": "string", "enum": kinds, "description": "The kind of hit search takes, song by default." },
                    "when": { "type": "string", "enum": ["now", "next", "last"], "default": "last" },
                },
            },
        },
        {
            "name": "providers",
            "description": "List the music providers, whether each has an account, and which is current.",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "switch_provider",
            "description": "Make another provider current, by the slug providers lists. It needs an account.",
            "inputSchema": {
                "type": "object",
                "properties": { "slug": { "type": "string" } },
                "required": ["slug"],
            },
        },
    ])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::commands::{Item, Kind, PlaybackCommand, QueueCommand, SearchQuery, Time};
    use crate::wire::{Failure, Value};

    /// Answers `line` with Sonora replying `reply`, and gives the reply and every request sent.
    fn call(line: Json, reply: Reply) -> (Option<Json>, Vec<Request>) {
        let mut sent = Vec::new();
        let answer = respond(&line.to_string(), &mut |request: &Request| {
            sent.push(request.clone());
            Ok(reply.clone())
        });
        (answer, sent)
    }

    fn tool(name: &str, arguments: Json) -> Json {
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
               "params": {"name": name, "arguments": arguments}})
    }

    #[test]
    fn initialize_names_sonora_and_offers_tools() {
        let (answer, sent) = call(
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                              "clientInfo": {"name": "test", "version": "0"}}}),
            Reply::Done,
        );
        let answer = answer.unwrap();
        assert_eq!(answer["id"], 1);
        assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(answer["result"]["serverInfo"]["name"], "sonora");
        assert!(answer["result"]["capabilities"]["tools"].is_object());
        assert!(sent.is_empty());
    }

    #[test]
    fn an_unknown_protocol_version_gets_the_newest_one_known() {
        let (answer, _) = call(
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "1999-01-01"}}),
            Reply::Done,
        );
        assert_eq!(answer.unwrap()["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn a_notification_gets_no_answer() {
        let (answer, _) = call(
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            Reply::Done,
        );
        assert_eq!(answer, None);
    }

    #[test]
    fn ping_answers_empty() {
        let (answer, _) = call(
            json!({"jsonrpc": "2.0", "id": "a", "method": "ping"}),
            Reply::Done,
        );
        assert_eq!(
            answer.unwrap(),
            json!({"jsonrpc": "2.0", "id": "a", "result": {}})
        );
    }

    #[test]
    fn an_unknown_method_is_refused() {
        let (answer, _) = call(
            json!({"jsonrpc": "2.0", "id": 2, "method": "nope"}),
            Reply::Done,
        );
        assert_eq!(answer.unwrap()["error"]["code"], -32601);
    }

    #[test]
    fn a_line_that_is_not_json_is_a_parse_error() {
        let answer = respond("{nope", &mut |_: &Request| Ok(Reply::Done)).unwrap();
        assert_eq!(answer["error"]["code"], -32700);
        assert_eq!(answer["id"], Json::Null);
    }

    #[test]
    fn every_tool_is_listed_with_an_object_schema() {
        let (answer, _) = call(
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            Reply::Done,
        );
        let answer = answer.unwrap();
        let tools = answer["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for name in [
            "now_playing",
            "playback",
            "seek",
            "volume",
            "shuffle",
            "repeat",
            "search",
            "queue",
            "providers",
            "switch_provider",
        ] {
            assert!(names.contains(&name), "{name} is not listed");
        }
        for tool in tools {
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(
                tool["description"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())
            );
        }
    }

    #[test]
    fn now_playing_answers_the_current_track_as_json() {
        let (answer, sent) = call(
            tool("now_playing", json!({})),
            Reply::Value(Value::Current(None)),
        );
        assert_eq!(sent, [Request::Playback(PlaybackCommand::CurrentTrack)]);
        let result = &answer.unwrap()["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "null");
    }

    #[test]
    fn playback_actions_reach_sonora() {
        let (_, sent) = call(tool("playback", json!({"action": "next"})), Reply::Done);
        assert_eq!(sent, [Request::Playback(PlaybackCommand::Next)]);
    }

    #[test]
    fn seek_takes_a_time_or_a_step() {
        let (_, sent) = call(tool("seek", json!({"to": 90})), Reply::Done);
        assert_eq!(
            sent,
            [Request::Playback(PlaybackCommand::SeekTime(Time::To(90)))]
        );
        let (_, sent) = call(tool("seek", json!({"by": -10})), Reply::Done);
        assert_eq!(
            sent,
            [Request::Playback(PlaybackCommand::SeekTime(Time::By(-10)))]
        );
    }

    #[test]
    fn search_defaults_to_ten_hits_of_every_kind() {
        let (_, sent) = call(
            tool("search", json!({"query": "daft punk"})),
            Reply::Value(Value::Hits(Vec::new())),
        );
        assert_eq!(
            sent,
            [Request::Search(SearchQuery {
                text: "daft punk".to_owned(),
                kind: None,
                limit: 10
            })]
        );
    }

    #[test]
    fn queue_plays_a_search_hit_now() {
        let (_, sent) = call(
            tool(
                "queue",
                json!({"search": "one more time", "type": "song", "when": "now"}),
            ),
            Reply::Done,
        );
        assert_eq!(
            sent,
            [Request::Queue(QueueCommand::Play(vec![Item::Search {
                text: "one more time".to_owned(),
                kind: Kind::Song,
            }]))]
        );
    }

    #[test]
    fn queue_adds_links_last_by_default() {
        let link = "https://open.spotify.com/track/abc";
        let (_, sent) = call(tool("queue", json!({"items": [link]})), Reply::Done);
        assert_eq!(
            sent,
            [Request::Queue(QueueCommand::Add(vec![Item::Link(
                link.to_owned()
            )]))]
        );
    }

    #[test]
    fn a_relative_path_is_refused_before_anything_is_sent() {
        let (answer, sent) = call(tool("queue", json!({"items": ["song.mp3"]})), Reply::Done);
        assert!(sent.is_empty());
        assert_eq!(answer.unwrap()["result"]["isError"], true);
    }

    #[test]
    fn a_refusal_from_sonora_is_a_tool_error_saying_why() {
        let (answer, _) = call(
            tool("switch_provider", json!({"slug": "deezer"})),
            Reply::Failed(Failure::NoAccount("Deezer".to_owned())),
        );
        let result = &answer.unwrap()["result"];
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Deezer")
        );
    }

    #[test]
    fn a_missing_argument_is_a_tool_error() {
        let (answer, sent) = call(tool("search", json!({})), Reply::Done);
        assert!(sent.is_empty());
        assert_eq!(answer.unwrap()["result"]["isError"], true);
    }

    #[test]
    fn an_unknown_tool_is_an_invalid_params_error() {
        let (answer, _) = call(tool("explode", json!({})), Reply::Done);
        assert_eq!(answer.unwrap()["error"]["code"], -32602);
    }

    #[test]
    fn sonora_not_running_is_a_tool_error() {
        let answer = respond(
            &tool("now_playing", json!({})).to_string(),
            &mut |_: &Request| Err(wire::Error::Io(io::Error::from(io::ErrorKind::NotFound))),
        )
        .unwrap();
        let result = &answer["result"];
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("not running")
        );
    }
}
