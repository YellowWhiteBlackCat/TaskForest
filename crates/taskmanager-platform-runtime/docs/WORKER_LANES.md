# Worker lanes and lifetime

## Key modules

- `src/channel/` and `src/delivery/` own bounded lanes, fairness and publication.
- `src/registration.rs` and `src/composition.rs` own provider catalog wiring.
- `src/config.rs` accepts required system/process bindings as named typed input
  transactions; lane promotion likewise consumes named observation/control
  groups. Positional multi-capability constructors are not a compatibility API.
- `src/assembly.rs` owns the one shared native provider/executor assembly seam;
  OS crates still own their registries, provider adaptation, and IDs.
- `src/ecs/lifecycle.rs` owns capability transitions; `src/ecs/target_jobs.rs`
  owns independently addressed job admission, leases, and entity retirement.
- `src/process.rs`, `src/system.rs`, `src/storage.rs` and `src/power.rs` own runtime jobs;
  `src/process/spawn.rs` isolates process worker startup and event routing from lane contracts.
  Directory scanning drives one typed lane context plus one immutable scan job.

## Worker lifetime boundary

Every provider lane is created through a named `std::thread::Builder` and is
owned by one `WorkerRuntime`; startup failure is typed and aborts native
composition. Seven dashboard lanes are resident; the remaining typed lanes
retain their bounded channel and provider state, start on first request, and
retire after 15 seconds idle. Detail loading is therefore on demand without
changing request, health, correlation, or failure semantics. A partial
composition drops the owner and disconnects idle lanes; the completed
`PlatformHandle` retains it opaquely through its last clone.

Worker admission has a 64-lane runtime ceiling and a 128-thread process ceiling.
Permits live on worker threads, so detached stuck providers remain bounded.
Normal exit returns the permit; drop wakes idle workers and joins only finished
threads. Native providers still own their I/O timeout/cancellation boundary,
and results returning after shutdown are not published.

Native bounded-command readers follow the same fallible named-thread rule:
partial startup reaps the child; sampler startup cleans up when its reader fails.
