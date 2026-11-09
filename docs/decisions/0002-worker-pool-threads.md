# 0002 - The worker pool keeps threads and a callback queue

Status: accepted, 2026-11-09, on the owner's word: "keep the threads as suggested".

## The question

`docs/decisions/0001-postgresql.md` changed the database engine and named one thing it did not
settle: how the pool talks to the engine. The reference's `DatabaseWorkerPool` owns N worker
threads, each with its own blocking connection, and callers hand work over a callback queue
(`DatabaseWorker`, `DatabaseWorkerPoolAdapter`, `QueryCallback`, `AdhocStatement`, `SQLOperation`).
Rust could replace that with an async pool and no queue at all.

## The decision

Keep the reference's arrangement: worker threads, one blocking connection per worker, and a
callback queue for the callers.

## Why

1. **The arrangement is what this project is porting.** The engine is the deviation recorded in
   0001; the shape of the pool is the reference's design, and `AGENTS.md` rule 1 says the
   reference decides design. An async pool would be a second deviation taken for convenience, in
   the layer that two programs depend on.
2. **The call sites stay recognisable.** In the reference a caller hands over a statement and a
   callback, or executes synchronously and waits. That is the same code shape whether the work
   happens on a thread or a runtime, so keeping the threads keeps the translation literal.
3. **It bounds the work.** A blocking client with one connection per worker has no cancellation,
   no runtime to drive and no task starvation to reason about. The behaviours to get right are
   ordering, the queue's shutdown and the connection's failure, which are exactly the behaviours
   the reference's code already demonstrates.

## What this means concretely

- `DatabaseWorkerPool` has N threads, each opening and owning one connection.
- Callers submit through the queue; a synchronous call still blocks the caller until its operation
  is done, as the reference's `DirectExecute` and `Query` do.
- The PostgreSQL client is used in its blocking form.
- A pool's shutdown drains the queue, then stops the threads, in that order.

## What is not decided here

The number of workers, their names, and the queue's capacity. Those are values, and values are
files: they arrive as keys in a program's configuration when the layer is written, with the
reference's own defaults named beside them.
