/// src/chatlogger.rs
/// Contains functions to manipulate Pokémon Showdown chatlogs stored in SQLite
///
/// Written by Annika

use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

use chrono::prelude::NaiveDateTime;
use rusqlite::{Connection, params};


#[derive(Debug)]
enum SQLParameter {
    Number(i32),
    Text(String),
    Null,
}

impl rusqlite::ToSql for SQLParameter {
    fn to_sql(&self) -> Result<rusqlite::types::ToSqlOutput, rusqlite::Error> {
        match self {
            Self::Number(n) => n.to_sql(),
            Self::Text(s) => s.to_sql(),
            Self::Null => rusqlite::types::Null.to_sql(),
        }
    }
}

/// Rustic version of the "userid|time|kind|senderName|body" log format
#[derive(Debug)]
pub struct LogEntry {
    time: i32,
    /// "chat" or "pm"
    kind: String,
    /// A Pokémon Showdown ID. See the PS source code for what this means.
    sender_id: String,
    sender_name: String,
    body: String,
}

/// Gets the number of seconds since the UNIX epoch as an i64
pub fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Logs a Pokémon Showdown chat message (or PM!) to a SQLite database.
///
/// # Example
/// ```
/// let message = chatlog_tools::LogEntry {
///     time: chatlog_tools::unix_time(),
///     kind: String::from("chat"),
///     sender_id: String::from("annika"),
///     sender_name: String::from("@Annika"),
///     body: String::from("hi"),
/// };
/// let my_connection = rusqlite::Connection::open_in_memory().unwrap()
///
/// chatlog_tools::log_message(my_conncetoin, "development", message);
/// ```
pub fn log_message(conn: &Connection, room: &str, message: LogEntry) -> Result<(), rusqlite::Error> {
    if message.kind != "chat" && message.kind != "pm" {
        return Ok(());
    }

    let mut statement = conn.prepare(
        "INSERT INTO logs (timestamp, userid, username, type, roomid, body) VALUES (?, ?, ?, ?, ?, ?)"
    )?;


    statement.execute(params![
        SQLParameter::Number(message.time),
        message.sender_id,
        message.sender_name,
        message.kind,
        room,
        message.body,
    ])?;

    Ok(())
}

/// Searches logs based on a variety of parameters.
/// Output is formatted as HTML suitable for a Pokémon Showdown HTML box
pub fn search(
    conn: &Connection, room_id: &str, user_id: Option<&str>,
    oldest: Option<i32>, keywords: Option<Vec<&str>>, max_messages: Option<i32>
) -> Result<String, rusqlite::Error> {
    let ranks = vec!['+', '^', '%', '@', '*', '#', '&', '~'];

    let mut query_str = String::from("SELECT * FROM logs WHERE roomid = ?");
    let mut args = Vec::<SQLParameter>::new();
    args.push(SQLParameter::Text(room_id.to_owned()));

    if let Some(id) = user_id {
        query_str.push_str(" AND userid = ?");
        args.push(SQLParameter::Text(id.to_owned()));
    }

    if let Some(keywords) = keywords {
        for keyword in keywords {
            query_str.push_str(" AND lower(body) LIKE '%' || ? || '%'");
            args.push(SQLParameter::Text(String::from(keyword).to_lowercase()));
        }
    }

    query_str.push_str(" AND timestamp > ? ORDER BY timestamp DESC LIMIT ?");
    args.push(SQLParameter::Number(oldest.unwrap_or(0)));
    args.push(SQLParameter::Number(max_messages.unwrap_or(1000)));

    let mut statement = conn.prepare(&query_str)?;

    // See https://github.com/hoodie/concatenation_benchmarks-rs for information on
    // string concatenation performance in Rust.
    // TL;DR .join()ing arrays or using push_str with a set-capacity String are best
    let mut html = String::with_capacity(100000);
    let mut rows = statement.query(args)?;
    let mut current_day = String::from("");
    while let Some(row) = rows.next()? {
        // row.get(1) -> timestamp
        let date = NaiveDateTime::from_timestamp(row.get(1).unwrap_or_else(|_| unix_time()), 0);
        let mdy = date.format("%v").to_string();
        if current_day != mdy {
            html.push_str(&[
                if !current_day.is_empty() {
                    "</div></details>"
                } else {
                    ""
                },
                r#"<details style="margin-left: 5px;"><summary><b>"#,
                &mdy,
                r#"</b></summary><div style="margin-left: 10px;">"#,
            ].join(""));
        }

        // row.get(3) -> sender_name
        let mut user: String = row.get(3)?;
        if ranks.contains(&user.chars().next().unwrap()) {
            user = [
                "<small>",
                &html_escape::encode_text(&user[0..1]),
                "</small><b>",
                &html_escape::encode_text(&user[1..]),
                "</b>",
            ].join("");
        } else {
            user = [
                "<b>",
                &user,
                "</b>",
            ].join("");
        }

        // format: <small>[XX:YY:ZZ]</small> user: xyzzy
        html.push_str(&[
            "<small>[",
            &html_escape::encode_text(&date.format("%T").to_string()),
            "] </small>",
            &user,
            ": ",
            &(row.get(6).unwrap_or_else(|_| String::from("")) as String)
        ].join(""));

        if current_day != mdy {
            current_day = mdy;
        }
    }
    if !current_day.is_empty() {
        html.push_str("</div></details>");
    }
    Ok(html)
}

pub fn get_linecount(conn: &Connection, user_id: &str, room_id: &str, days: Option<i32>) -> Result<i32, rusqlite::Error> {
    let days = days.unwrap_or(30);

    let max_timestamp = unix_time() - (days * 24 * 60 * 60) as i64;
    let mut statement = conn.prepare("SELECT count(log_id) FROM logs WHERE userid = ? AND roomid = ? AND timestamp > ?")?;
    statement.query_row(params![user_id, room_id, max_timestamp], |row| row.get(0))
}

/// Gets the users with the highest linecount in a room
/// Returns a Result<HashMap<user ID, linecount>>
pub fn get_topusers(
    conn: &Connection, room_id: &str, days: Option<i32>, num_users: Option<i32>
) -> Result<HashMap<String, i32>, rusqlite::Error> {
    Ok(HashMap::new())
}

/// Gets the users with the highest linecount in a room and formats them as HTML
/// Returns a Result<HashMap<user ID, linecount>>
pub fn get_topusers_html(
    conn: &Connection, room_id: &str, days: Option<i32>, num_users: Option<i32>
) -> Result<String, rusqlite::Error> {
    Ok("".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn get_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute(
            "CREATE TABLE IF NOT EXISTS logs (
                log_id INTEGER NOT NULL PRIMARY KEY,
                -- UNIX timestamp
                timestamp INTEGER NOT NULL,
                -- the ID of the user who sent the message
                userid TEXT,
                -- their name
                username TEXT,
                -- chat, pm, etc
                type TEXT NOT NULL,
                -- null if it's a PM/other global message
                roomid TEXT,
                body TEXT
            );

            CREATE INDEX IF NOT EXISTS log_index_1 ON logs(timestamp);
            CREATE INDEX IF NOT EXISTS log_index_2 ON logs(userid, timestamp);
            CREATE INDEX IF NOT EXISTS log_index_3 ON logs(userid, roomid, timestamp);
            CREATE INDEX IF NOT EXISTS log_index_5 ON logs(type, userid, roomid, timestamp);
            PRAGMA journal_mode=WAL;",
            rusqlite::NO_PARAMS
        ).unwrap();
        connection
    }

    fn add_test_data(conn: &Connection, current_time: i32) -> Result<(), rusqlite::Error> {
        log_message(conn, "test", LogEntry {
            body: String::from("Test Message One"),
            kind: String::from("chat"),
            sender_id: String::from("annika"),
            sender_name: String::from("@Annika"),
            time: current_time,
        })?;
        log_message(conn, "test", LogEntry {
            body: String::from("Test Message Two"),
            kind: String::from("chat"),
            sender_id: String::from("annika"),
            sender_name: String::from("@Annika"),
            time: current_time,
        })?;
        log_message(conn, "test", LogEntry {
            body: String::from("Test Message Three"),
            kind: String::from("chat"),
            sender_id: String::from("annika"),
            sender_name: String::from("@Annika"),
            time: current_time - 15 * 30 * 60 * 60, // 15 days ago
        })?;
        log_message(conn, "test", LogEntry {
            body: String::from("Test Message Four"),
            kind: String::from("chat"),
            sender_id: String::from("heartofetheria"),
            sender_name: String::from("Heart of Etheria"),
            time: current_time - 15 * 30 * 60 * 60, // 15 days ago
        })?;

        Ok(())
    }

    #[test]
    fn insertion_test() -> Result<(), rusqlite::Error> {
        let conn = get_connection();

        log_message(&conn, "test", LogEntry {
            body: String::from("Hello from Rust!"),
            kind: String::from("chat"),
            sender_id: String::from("annika"),
            sender_name: String::from("@Annika"),
            time: 1601875655,
        })?;

        conn.query_row("SELECT * FROM logs", rusqlite::NO_PARAMS, |row: &rusqlite::Row| {
            assert_eq!(row.get::<usize, i32>(1).unwrap(), 1601875655);
            assert_eq!(row.get::<usize, String>(2).unwrap(), String::from("annika"));
            assert_eq!(row.get::<usize, String>(3).unwrap(), String::from("@Annika"));
            assert_eq!(row.get::<usize, String>(4).unwrap(), String::from("chat"));
            assert_eq!(row.get::<usize, String>(5).unwrap(), String::from("test"));
            assert_eq!(row.get::<usize, String>(6).unwrap(), String::from("Hello from Rust!"));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn search_test() -> Result<(), rusqlite::Error> {
        let conn = get_connection();
        add_test_data(&conn, 1602131140)?;

        // Check that it can search by user ID and format regular users
        let mut results = search(&conn, "test", Some("heartofetheria"), None, None, None)?;
        // 19 Sep = 15 days ago as per add_test_data()
        assert_eq!(results, "<details style=\"margin-left: 5px;\"><summary><b>19-Sep-2020</b></summary><div style=\"margin-left: 10px;\"><small>[10:25:40] </small><b>Heart of Etheria</b>: Test Message Four</div></details>");

        // Check that it can format auth correctly
        results = search(&conn, "test", Some("annika"), Some(0), None, Some(1))?;
        assert_eq!(results, "<details style=\"margin-left: 5px;\"><summary><b> 8-Oct-2020</b></summary><div style=\"margin-left: 10px;\"><small>[04:25:40] </small><small>@</small><b>Annika</b>: Test Message One</div></details>");

        // Check that it can search by time
        results = search(&conn, "test", None, Some(1602131140 - 100), None, Some(1000))?;
        assert_eq!(results.contains("Test Message One"), true);
        assert_eq!(results.contains("Test Message Two"), true);
        assert_eq!(results.contains("Test Message Three"), false);
        assert_eq!(results.contains("Test Message Four"), false);

        // Check that it can limit the number of messages returned
        results = search(&conn, "test", None, None, None, Some(1))?;
        assert_eq!(results.contains("Test Message One"), true);
        assert_eq!(results.contains("Test Message Two"), false);
        assert_eq!(results.contains("Test Message Three"), false);
        assert_eq!(results.contains("Test Message Four"), false);

        // Check that it can search by a (case-insensitive) keyword
        results = search(&conn, "test", None, None, Some(vec!["tWo"]), None)?;
        assert_eq!(results.contains("Test Message One"), false);
        assert_eq!(results.contains("Test Message Two"), true);
        assert_eq!(results.contains("Test Message Three"), false);
        assert_eq!(results.contains("Test Message Four"), false);

        Ok(())
    }

    #[test]
    fn linecount_test() -> Result<(), rusqlite::Error> {
        let conn = get_connection();
        add_test_data(&conn, unix_time() as i32)?;

        // Test that it works
        assert_eq!(get_linecount(&conn, "annika", "test", None), Ok(3));
        assert_eq!(get_linecount(&conn, "heartofetheria", "test", None), Ok(1));

        // Test that it limits the number of days
        assert_eq!(get_linecount(&conn, "annika", "test", Some(10)), Ok(2));
        assert_eq!(get_linecount(&conn, "heartofetheria", "test", Some(10)), Ok(0));

        Ok(())
    }

    #[test]
    fn topusers_test() -> Result<(), rusqlite::Error> {
        let conn = get_connection();
        add_test_data(&conn, unix_time() as i32)?;

        // test that it works
        let mut topusers = get_topusers(&conn, "test", None, None)?;
        assert_eq!(topusers.get("annika"), Some(&3));
        assert_eq!(topusers.get("heartofetheria"), Some(&1));

        // test that it limits by day
        topusers = get_topusers(&conn, "test", Some(1), None)?;
        assert_eq!(topusers.get("annika"), Some(&2));
        assert_eq!(topusers.get("heartofetheria"), None);

        // test that it limits by number of users
        topusers = get_topusers(&conn, "test", None, Some(1))?;
        assert_eq!(topusers.get("annika"), Some(&3));
        assert_eq!(topusers.get("heartofetheria"), None);

        Ok(())
    }

    #[test]
    fn topusers_html_test() -> Result<(), rusqlite::Error> {
        let conn = get_connection();
        add_test_data(&conn, unix_time() as i32)?;

        let html = get_topusers_html(&conn, "test", None, None)?;
        assert_eq!(html, r#"<details><summary>Top users in the room test</summary><ul><li><strong>annika</strong> — 3 lines</li><li><strong>heartofetheria</strong> — 1 line</li></ul></details>"#);

        Ok(())
    }
}