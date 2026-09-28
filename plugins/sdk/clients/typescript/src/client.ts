import { PluginSdkHost } from './host.js';
import { decode, encode } from '@msgpack/msgpack';

/** One Core watch event returned through Plugin SDK IPC. */
export interface OperitPluginSdkEvent {
  readonly requestId: string | null;
  readonly target: string;
  readonly propertyName: string;
  readonly kind: string;
  readonly value: unknown;
}

/** One caller-owned Core push stream. */
export interface OperitPluginSdkPushSink {
  add(value: unknown): Promise<void>;
  close(): Promise<void>;
}

/** Concrete Node.js Plugin SDK client with its IPC carrier built in. */
export class OperitPluginSdkClient {
  private constructor(private readonly host: PluginSdkHost) {
    host.on('message', (chunk) => this.onBytes(chunk));
    host.on('failure', (error) => this.fail(error));
    host.on('closed', () => this.fail(new Error('Plugin SDK IPC connection closed')));
  }

  private readonly pending = new Map<string, (value: unknown, error?: Error) => void>();
  private readonly watchQueues = new Map<string, AsyncQueue<OperitPluginSdkEvent>>();
  private closed = false;
  private nextId = 1;

  /** Activates Operit using the platform host packaged with this SDK. */
  public static async connect(nativeLibraryPath?: string): Promise<OperitPluginSdkClient> {
    return new OperitPluginSdkClient(await PluginSdkHost.connect(nativeLibraryPath));
  }

  /** Closes the native session and fails outstanding operations. */
  public close(): void { this.host.close(); this.fail(new Error('Plugin SDK connection closed')); }

  /** Calls one generated Core method. */
  public call(target: string, methodName: string, args?: unknown): Promise<unknown> {
    const requestId = this.requestId();
    return this.request(requestId, {
      type: 'Call',
      body: { requestId, target, methodName, args: args ?? {} },
    });
  }

  /** Calls one route and decodes its MessagePack value. */
  public callTyped<T>(target: string, methodName: string, args: unknown, decodeValue: (value: unknown) => T): Promise<T> {
    return this.call(target, methodName, args).then(decodeValue);
  }

  /** Watches one generated Core property. */
  public async *watch(target: string, propertyName: string, args?: unknown): AsyncIterable<OperitPluginSdkEvent> {
    const subscriptionId = this.requestId();
    const queue = new AsyncQueue<OperitPluginSdkEvent>();
    this.watchQueues.set(subscriptionId, queue);
    this.send({
      type: 'WatchOpen',
      body: {
        subscriptionId,
        request: { requestId: subscriptionId, target, propertyName, args: args ?? {} },
      },
    });
    try {
      for await (const event of queue) yield event;
    } finally {
      this.watchQueues.delete(subscriptionId);
      this.send({ type: 'WatchClose', body: { subscriptionId, error: null } });
    }
  }

  /** Watches one route and decodes each MessagePack event value. */
  public async *watchTyped<T>(target: string, propertyName: string, args: unknown, decodeValue: (value: unknown) => T): AsyncIterable<T> {
    for await (const event of this.watch(target, propertyName, args)) yield decodeValue(event.value);
  }

  /** Opens one generated caller-owned Core input stream. */
  public async push(target: string, methodName: string, args?: unknown): Promise<OperitPluginSdkPushSink> {
    const pushId = this.requestId();
    await this.request(pushId, {
      type: 'PushOpen',
      body: { requestId: pushId, target, methodName, args: args ?? {} },
    });
    let sequence = 0;
    return {
      add: (value) => this.request(`${pushId}:${sequence}`, {
        type: 'PushItem',
        body: { pushId, sequence: sequence++, args: value },
      }).then(() => undefined),
      close: () => this.request(pushId, { type: 'PushClose', body: { pushId } }).then(() => undefined),
    };
  }

  /** Allocates one session-local request id. */
  private requestId(): string { return `plugin-sdk-${this.nextId++}`; }

  /** Sends one envelope through the host-owned framing implementation. */
  private send(message: unknown): void {
    const payload = Buffer.from(encode(message));
    if (this.closed) throw new Error('Plugin SDK connection closed');
    this.host.send(payload);
  }

  /** Registers and sends one request awaiting its protocol acknowledgement. */
  private request(key: string, message: unknown): Promise<unknown> {
    const promise = new Promise<unknown>((resolve, reject) => {
      this.pending.set(key, (value, error) => error ? reject(error) : resolve(value));
    });
    try { this.send(message); } catch (error) { this.fail(error as Error); }
    return promise;
  }

  /** Decodes one complete envelope received from the platform host. */
  private onBytes(chunk: Buffer): void {
    try { this.onMessage(decode(chunk) as Record<string, unknown>); }
    catch (error) { this.fail(error as Error); this.host.close(); }
  }

  /** Routes one protocol envelope to its pending request or watch queue. */
  private onMessage(message: Record<string, unknown>): void {
    const body = (message.body ?? {}) as Record<string, unknown>;
    switch (message.type) {
      case 'CallResponse': this.complete(String(body.requestId), this.result(body.result)); break;
      case 'WatchEvent': this.watchQueues.get(String(body.subscriptionId))?.push(this.event(body.event)); break;
      case 'WatchClose': this.watchQueues.get(String(body.subscriptionId))?.close(); break;
      case 'WatchOpened': this.complete(String(body.subscriptionId), this.result(body.result)); break;
      case 'PushOpened': this.complete(String(body.pushId), this.result(body.result)); break;
      case 'PushItemResult': this.complete(`${body.pushId}:${body.sequence}`, this.result(body.result)); break;
      case 'PushClosed': this.complete(String(body.pushId), this.result(body.result)); break;
      case 'ProtocolError': this.fail(this.error(body.error)); break;
    }
  }

  /** Completes one registered protocol request. */
  private complete(key: string, result: unknown): void {
    const callback = this.pending.get(key);
    if (!callback) return;
    this.pending.delete(key);
    callback(result);
  }

  /** Converts a serialized Result into a value or exception. */
  private result(value: unknown): unknown {
    if (value && typeof value === 'object' && 'Err' in value) throw this.error((value as { Err: unknown }).Err);
    return value && typeof value === 'object' && 'Ok' in value ? (value as { Ok: unknown }).Ok : value;
  }

  /** Converts one serialized Core error. */
  private error(value: unknown): Error { return new Error(String((value as { message?: unknown })?.message ?? value)); }

  /** Converts one serialized Core event. */
  private event(value: unknown): OperitPluginSdkEvent {
    const event = value as Record<string, unknown>;
    return { requestId: event.requestId as string | null, target: event.target as string, propertyName: event.propertyName as string, kind: String(event.kind), value: event.value };
  }

  /** Fails pending calls and watch streams after the platform carrier closes. */
  private fail(error: Error): void {
    this.closed = true;
    for (const callback of this.pending.values()) callback(undefined, error);
    this.pending.clear();
    for (const queue of this.watchQueues.values()) queue.fail(error);
    this.watchQueues.clear();
  }
}

/** Minimal async queue used by generated watch wrappers. */
class AsyncQueue<T> implements AsyncIterable<T>, AsyncIterator<T> {
  private readonly values: T[] = [];
  private readonly waiters: Array<{ resolve: (result: IteratorResult<T>) => void; reject: (error: Error) => void }> = [];
  private done = false;
  private failure?: Error;
  /** Adds one queue value. */
  public push(value: T): void { if (this.done) return; const waiter = this.waiters.shift(); waiter ? waiter.resolve({ value, done: false }) : this.values.push(value); }
  /** Closes the queue. */
  public close(): void { this.done = true; while (this.waiters.length) this.waiters.shift()!.resolve({ value: undefined as never, done: true }); }
  /** Fails an active watch when its native transport is lost. */
  public fail(error: Error): void { this.done = true; this.failure = error; this.values.length = 0; while (this.waiters.length) this.waiters.shift()!.reject(error); }
  /** Reads one queue value. */
  public next(): Promise<IteratorResult<T>> { if (this.failure) return Promise.reject(this.failure); if (this.values.length) return Promise.resolve({ value: this.values.shift()!, done: false }); if (this.done) return Promise.resolve({ value: undefined as never, done: true }); return new Promise((resolve, reject) => this.waiters.push({ resolve, reject })); }
  /** Returns this queue as its async iterator. */
  public [Symbol.asyncIterator](): AsyncIterator<T> { return this; }
}
