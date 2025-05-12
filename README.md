# RFS Runner (UI Progress boilerplate)

my helper

## Features

* Manages a pool of asynchronous workers.
* Supports sending data to workers sequentially or to specific workers by ID.
* Provides progress reporting capabilities using [indicatif](https://docs.rs/indicatif).
* Handles graceful shutdown on SIGINT with `WorkerPool::sigint` hook.

**Note:** You'll need to replace `rfs-runner` with the actual name of your crate and ensure `WorkerPool`, `DefaultTemplate`, and `MainProgress` are correctly exposed from your library's public API. The example assumes `DefaultTemplate` is a struct implementing `WorkerTemplate`.

## API

Key components:

* **`WorkerPool<D, S>`**: Manages the workers.
    * `new(num_cpus: usize, template: S)`: Creates a new pool.
    * `spawn_worker<F, Fut>(&mut self, f: F) -> Uid`: Spawns a new worker.
    * `send_seqcst(&mut self, data: D) -> Result<(), D>`: Sends data to the next available worker.
    * `send_to(&mut self, id: Uid, data: D) -> Result<(), D>`: Sends data to a specific worker.
    * `join_all(self)`: Waits for all workers to finish.
    * `sigint(&mut self)`: Handles SIGINT for graceful shutdown.
* **`WorkerTemplate` (trait)**: Defines the styling for progress bars.
    * `DefaultTemplate` is an example implementation.
* **`MainProgress<S>`**: Handles progress bar creation and updates.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

Distributed under the [MIT License](LICENSE). See `LICENSE` for more information.

Project Link: [rfs-runer](https://github.com/rootfebri/rfs-runner)