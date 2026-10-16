# How marcr::parallel works

`marcr::parallel::convert` (see the
[README](../README.md#parallel-processing) for its use) runs three stages
linked by bounded queues. The code is in
[src/parallel.rs](../src/parallel.rs).

```mermaid
flowchart LR
    IN[("Input<br/>Reader")]

    subgraph S1["Stage 1: splitter thread"]
        SPLIT["split()<br/>reader.extract() delimits<br/>raw records, no parsing"]
    end

    Q1[["sync_channel, 4 batches"]]

    subgraph S2["Stage 2: calling thread + rayon pool"]
        LOOP["for batch in batch_rx"]
        subgraph RAYON["into_par_iter() over the batch records"]
            direction TB
            P1["deserialize → process → serialize"]
            P2["deserialize → process → serialize"]
            P3["…"]
        end
        COLLECT["collect()<br/>outcomes in input order"]
        LOOP --> RAYON --> COLLECT
    end

    Q2[["sync_channel, 4 batches"]]

    subgraph S3["Stage 3: output thread"]
        WRITE["write()"]
        OK["Ok(Some) → write_serialized()<br/>written += 1"]
        NONE["Ok(None) → filtered out"]
        ERR["Err → on_skip(number, error)<br/>skipped += 1"]
        WRITE --> OK & NONE & ERR
    end

    OUT[("Output<br/>Writer")]

    IN --> SPLIT -->|"batch of ~4 MB<br/>data + ends + positions"| Q1 --> LOOP
    COLLECT -->|"batch + outcomes"| Q2 --> WRITE
    OK --> OUT
```

- Stage 1 only finds where each record ends; parsing, the `process`
  function and serialization, the costly parts, all run in stage 2, on
  all cores.
- Rayon processes the records of a batch in any order, but `collect()`
  puts their outcomes back in input order, and batches go through the
  queues one after the other: hence the same output as a sequential loop.
- The queues are bounded: a stage that gets ahead blocks until the next
  one catches up, so only about ten batches are in memory at once.

There is no shared stop flag: a stage that stops drops its end of a
queue, and the others stop when they can no longer send or receive:

```mermaid
sequenceDiagram
    participant S as Stage 1 (split)
    participant P as Stage 2 (rayon)
    participant W as Stage 3 (write)

    Note over S,W: Normal end
    S->>P: last batch
    S--xP: empty batch → return Ok, batch_tx dropped
    P->>W: last outcomes
    P--xW: loop ends → drop(done_tx)
    W-->>P: Ok(Stats) on join()

    Note over S,W: I/O error on output
    W--xP: write() returns Err, done_rx dropped
    P--xS: done_tx.send() fails → break, batch_rx dropped
    S-->>P: batch_tx.send() fails → return Ok
    Note over P: join() both threads → return the write error

    Note over S,W: I/O error on input
    S--xP: split() returns Err, batch_tx dropped
    P--xW: loop ends, batches already read are written
    Note over P: join() both threads → return the split error
```
