/**
 * Gateway client for connecting to Tor relays via WebSocket or WebRTC.
 *
 * The Gateway tries WebRTC first (multiplexed data channels over a single
 * peer connection), then falls back to WebSocket (one connection per relay).
 *
 * Each `connect(target)` call returns a {@link RelaySocket} — a uniform
 * bidirectional byte pipe regardless of transport.
 */

// ---------------------------------------------------------------------------
// RelaySocket — uniform socket interface
// ---------------------------------------------------------------------------

/**
 * A bidirectional byte pipe to a Tor relay.
 *
 * Assign `onmessage` and `onclose` after creation.
 * Call `send(data)` with Uint8Array and `close()` when done.
 */
export class RelaySocket {
  #send: (data: Uint8Array) => void;
  #close: () => void;
  #closed = false;
  #onclose: (() => void) | null = null;

  /** Set by transport on error, before onclose fires. */
  _error: string | null = null;

  /** Receive callback — transport fires this with each incoming chunk. */
  onmessage: ((data: Uint8Array) => void) | null = null;

  constructor(
    send: (data: Uint8Array) => void,
    close: () => void,
  ) {
    this.#send = send;
    this.#close = close;
  }

  /** Setter that fires immediately if close already happened. */
  set onclose(fn: (() => void) | null) {
    this.#onclose = fn;
    if (this.#closed && fn) queueMicrotask(() => fn());
  }
  get onclose(): (() => void) | null { return this.#onclose; }

  /** @internal — called by transport wrappers when the underlying connection closes. */
  _notifyClose(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#onclose?.();
  }

  send(data: Uint8Array): void {
    this.#send(data);
  }

  close(): void {
    this.#close();
  }

  // -- Transport factories --------------------------------------------------

  /** Wrap a browser WebSocket (already open). */
  static fromWebSocket(ws: WebSocket): RelaySocket {
    const sock = new RelaySocket(
      (data) => ws.send(data),
      () => ws.close(),
    );
    ws.onmessage = (ev) => sock.onmessage?.(new Uint8Array(ev.data));
    ws.onclose = () => sock._notifyClose();
    return sock;
  }

  /** Wrap a WebRTC data channel (already open). */
  static fromDataChannel(dc: RTCDataChannel): RelaySocket {
    const sock = new RelaySocket(
      (data) => dc.send(data),
      () => dc.close(),
    );
    dc.onmessage = (ev) => sock.onmessage?.(new Uint8Array(ev.data));
    dc.onclose = () => sock._notifyClose();
    return sock;
  }
}

// ---------------------------------------------------------------------------
// Gateway — multi-strategy connection manager
// ---------------------------------------------------------------------------

/**
 * Options for creating a Gateway.
 */
export interface GatewayOptions {
  /**
   * Ordered list of strategies to try: `"webrtc"`, `"websocket"`.
   * Defaults to `["webrtc", "websocket"]` in browsers with WebRTC,
   * or `["websocket"]` otherwise.
   */
  strategies?: string[];
}

interface TrackedEntry {
  dc: RTCDataChannel;
  sock: RelaySocket | null;
  reject: ((err: Error) => void) | null;
}

/**
 * Gateway client. Opens relay sockets via configurable strategies
 * (WebRTC data channels, WebSocket) with automatic fallback.
 */
export class Gateway {
  #url: string;
  #strategies: string[];

  // WebRTC state (lazily created, reused across connect() calls)
  #rtcPc: RTCPeerConnection | null = null;
  #rtcAlive = false;
  #signalChannel: RTCDataChannel | null = null;
  // Tracked data channels: before open has reject, after open has sock.
  #tracked: TrackedEntry[] = [];

  constructor(url: string, options: GatewayOptions = {}) {
    this.#url = url.replace(/\/+$/, '');
    this.#strategies = options.strategies ?? defaultStrategies();
  }

  /**
   * Open a relay socket to the given target (e.g. "198.51.100.1:9001").
   * Tries each configured strategy in order until one succeeds.
   */
  async connect(target: string): Promise<RelaySocket> {
    const errors: string[] = [];

    for (const strategy of this.#strategies) {
      try {
        switch (strategy) {
          case 'webrtc':
            return await this.#connectWebRTC(target);
          case 'websocket':
            return await this.#connectWebSocket(target);
          default:
            throw new Error(`unknown strategy: ${strategy}`);
        }
      } catch (e: any) {
        errors.push(`${strategy}: ${e.message}`);
      }
    }

    throw new Error(`all strategies failed for ${target}: ${errors.join('; ')}`);
  }

  /** Close WebRTC peer connection and release resources. */
  close(): void {
    if (this.#rtcPc) {
      this.#rtcPc.close();
      this.#rtcPc = null;
      this.#rtcAlive = false;
      this.#signalChannel = null;
    }
  }

  // -- WebSocket strategy ---------------------------------------------------

  async #connectWebSocket(target: string): Promise<RelaySocket> {
    const wsUrl = `${this.#url.replace(/^http/, 'ws')}/socket/${target}`;
    const ws = new WebSocket(wsUrl);
    ws.binaryType = 'arraybuffer';

    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error('websocket connection failed'));
    });

    return RelaySocket.fromWebSocket(ws);
  }

  // -- WebRTC strategy ------------------------------------------------------

  async #connectWebRTC(target: string): Promise<RelaySocket> {
    if (typeof RTCPeerConnection === 'undefined') {
      throw new Error('RTCPeerConnection not available');
    }

    // Create or reuse peer connection
    if (!this.#rtcAlive) {
      if (this.#rtcPc) this.#rtcPc.close();
      await this.#setupRtcPeerConnection();
    }

    const dc = this.#rtcPc!.createDataChannel(target);
    dc.binaryType = 'arraybuffer';

    const entry = { dc, sock: null as RelaySocket | null, reject: null as ((err: Error) => void) | null };
    this.#tracked.push(entry);

    // Race: channel opens vs server rejects via _signal
    await new Promise<void>((resolve, reject) => {
      entry.reject = reject;
      dc.onopen = () => resolve();
      dc.onerror = (e: any) => {
        this.#removeTracked(entry);
        reject(new Error(`data channel error: ${e.error?.message || e}`));
      };
    });

    // Channel is open — dc.id now available
    entry.reject = null;
    const sock = RelaySocket.fromDataChannel(dc);
    entry.sock = sock;
    dc.onclose = () => {
      this.#removeTracked(entry);
      sock._notifyClose();
    };
    return sock;
  }

  async #setupRtcPeerConnection(): Promise<void> {
    const pc = new RTCPeerConnection();

    // Signal channel for control messages (hello, ping/pong, rejections)
    const signal = pc.createDataChannel('_signal');
    signal.onmessage = (ev) => this.#handleSignalMessage(ev.data);

    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);

    // Wait for ICE gathering to complete
    await new Promise<void>((resolve) => {
      if (pc.iceGatheringState === 'complete') return resolve();
      pc.addEventListener('icegatheringstatechange', () => {
        if (pc.iceGatheringState === 'complete') resolve();
      });
    });

    const res = await fetch(`${this.#url}/rtc/connect`, {
      method: 'POST',
      body: JSON.stringify(pc.localDescription),
    });

    if (!res.ok) {
      pc.close();
      throw new Error(`rtc signaling failed: ${res.status} ${await res.text()}`);
    }

    const answer = await res.json();
    await pc.setRemoteDescription(answer);

    // Wait for connection
    await new Promise<void>((resolve, reject) => {
      if (pc.connectionState === 'connected') return resolve();
      pc.addEventListener('connectionstatechange', () => {
        if (pc.connectionState === 'connected') resolve();
        if (pc.connectionState === 'failed') reject(new Error('WebRTC connection failed'));
      });
    });

    this.#rtcPc = pc;
    this.#rtcAlive = true;
    this.#signalChannel = signal;

    pc.addEventListener('connectionstatechange', () => {
      const s = pc.connectionState;
      if (s === 'disconnected' || s === 'closed' || s === 'failed') {
        this.#rtcAlive = false;
        this.#signalChannel = null;
      }
    });
  }

  #findTracked(sctpId: number | null, label: string) {
    return this.#tracked.find(e => e.dc.id != null && e.dc.id === sctpId)
      ?? this.#tracked.find(e => e.dc.label === label);
  }

  #removeTracked(entry: TrackedEntry): void {
    const i = this.#tracked.indexOf(entry);
    if (i !== -1) this.#tracked.splice(i, 1);
  }

  #handleSignalMessage(data: string): void {
    try {
      const msg = JSON.parse(data);
      switch (msg.type) {
        case 'rejected': {
          const entry = this.#findTracked(msg.sctp_id, msg.channel);
          if (entry) {
            this.#removeTracked(entry);
            if (entry.reject) {
              entry.reject(new Error(`rejected: ${msg.reason}`));
            } else if (entry.sock) {
              entry.sock._error = msg.reason;
              entry.sock.close();
              entry.sock._notifyClose();
            }
          }
          break;
        }
        case 'closed': {
          const entry = this.#findTracked(msg.sctp_id, msg.channel);
          if (entry) {
            this.#removeTracked(entry);
            if (entry.sock) {
              entry.sock.close();
              entry.sock._notifyClose();
            }
          }
          break;
        }
      }
    } catch { /* ignore malformed signal messages */ }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function defaultStrategies(): string[] {
  const s: string[] = [];
  if (typeof RTCPeerConnection !== 'undefined') s.push('webrtc');
  if (typeof WebSocket !== 'undefined') s.push('websocket');
  return s;
}
