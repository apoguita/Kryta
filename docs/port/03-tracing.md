# What the server tells you, and how to switch on the rest

A statement of today. Every line here is something the running programs print or can be made to
print; nothing here is a plan.

The rule behind all of it: a server that cannot say whether anything reached it, or what it did with
what reached it, cannot be tested by anyone - not by the person running it, and not by an operator
reading its log afterwards.

## Always on

The startup reports each stage as it happens, and then says what state it is in:

```text
authserver: config authserver\authserver.conf (18 keys), would bind 0.0.0.0:6112
> Using configuration file       authserver\authserver.conf
Kryta authserver 0.1.0
Opening DatabasePool 'kryta_login'. Asynchronous connections: 2, synchronous connections: 1.
Login database: 0 file(s) applied, 2 already current.
Started auth database connection pool.
World list: 1 world(s), refreshed every 20 s.
File check: 21 bytes of question, 32 of answer.
listening on 0.0.0.0:6112 with 1 network thread(s)
READY: listening on 0.0.0.0:6112; status reported every 5 s.
```

Then, while it runs:

| line | when |
|---|---|
| `accepted connection #N from ADDRESS` | every connection, the moment it is accepted, before any protocol sees it |
| `Connection from ADDRESS: auth version N` | the client said what build it is |
| `Connection from ADDRESS: file check answered with 32 bytes` | the client's file check was answered |
| `Connection from ADDRESS: the plaintext half of the exchange is answered` | the seed exchange completed |
| `Connection from ADDRESS: first encrypted message is 0x8038 ... the key derivation matches` | the client's first scrambled message read back as the login message |
| `status: up N s, listening on ADDRESS:PORT, N connection(s) accepted` | every `StatusInterval` seconds |
| `Portal: listening on ADDRESS:PORT.` | when the portal is enabled, on the address it bound |
| `ADDRESS: GET /Spawned/WebGate/session/create.xml -> 200 (N bytes)` | one line per portal request, with its status |
| `Stopped. N connection(s) were accepted while it ran.` | on shutdown |

A connection that says nothing produces `accepted connection` and then nothing else. That pair -
accepted, silent - is the difference between "the client reached us and gave up" and "the client
never reached us", which is the question that could not be answered before.

## Optional traces

Both are off by default, both are one key, and both need their own logger - the bytes and the
statements are written at debug, and a switch that silently does nothing is worse than no switch. If
one is on and its logger is not, the program says so and names the line to add.

| what | key | logger line to add | what it prints |
|---|---|---|---|
| every byte of every connection | `Network.Trace = 1` | `Logger.network.trace = 6,Console Auth` | `trace ADDRESS in  4 bytes: 00 04 0C 00` and the outgoing side, plus a close line with the totals |
| every database statement | `Database.Trace = 1` | `Logger.sql.trace = 6,Console Auth` | `query [database] SELECT ...` and `rows [database] 1`, `batch`, `execute`, `affected` |
| every key the configuration held | `Log.EffectiveConfig = 1` | `Logger.server.loading = 6,Console Auth` | `> Config: LoginDatabaseInfo = 127.0.0.1;5432;kryta;********;kryta_login` - a connection string's password is masked, because a log line gets pasted into issue reports |

The byte trace is the one that turns an unexplained client into evidence: whatever a client sends,
including traffic this server does not understand, appears byte for byte with its direction. The
statement trace is the same idea for the database: a startup that applied the wrong file looks like
a startup that worked until the statements are visible.

## Reading a failure

| what you see | what it means | where to look |
|---|---|---|
| no `accepted connection` line at all | nothing reached the port: the client is dialling elsewhere, or nothing is listening | check the `READY` line's address and port; then the client's own redirect |
| `accepted connection` then nothing | the client connected and sent nothing, or closed | is a trace on? if so the close line says how many bytes arrived |
| `... is neither the file check nor a connection this server recognises` | the first bytes are not something this server knows | `data/file_check.conf` and the three connection headers |
| `started like the file check but is not it` | the first four bytes matched and the rest did not | the trace's bytes against the file's 21 |
| `sent a seed with header 0xXXXX` | the client's seed is not the shape expected | the exchange's sizes |
| `first encrypted message reads as 0xXXXX ... the login message is 0x8038` | the derivation, or the framing after it, is wrong | the trace's bytes for the first scrambled message |

## Where it goes

The `LogsDir` key decides where file appenders write; empty means the working directory. The shipped
configuration writes `Auth.log` for the auth program and `Server.log` and `Errors.log` for the world
program, and the console appender writes the same lines to the terminal.
