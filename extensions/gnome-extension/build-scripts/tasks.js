/** @public @template T @class */
export class Task {
  /** @private @property {Promise<T>} */
  #promise;

  /** @public @constructor @param {() => Promise<T>} spawner */
  constructor(spawner) {
    this.#promise = spawner();
  }

  /** @public @async @method @returns {Promise<T>} */
  wait() {
    return this.#promise;
  }
}

/** @public @class */
export class TaskSet {
  /** @private @property @type {Set<Promise<any>>} */
  #tasks = new Set();
  /** @private @property @type {Promise<void>|null} */
  #waiting = null;

  /** @public @method @param {Task<any>} task @returns {void} */
  add(task) {
    this.#tasks.add(task.wait());
  }

  /** @public @async @method @returns {Promise<void>} */
  async wait() {
    if (this.#waiting !== null) return this.#waiting;

    await (this.#waiting = new Task(async () => {
      while (true) {
        const snapshot = new Set(this.#tasks);
        await Promise.all(snapshot);
        if (snapshot.size === this.#tasks.size) break;
      }
      this.#tasks.clear();
      this.#waiting = null;
    }).wait());
  }
}
