/** Original beds and one waltz, synthesized in the browser. Nothing here is a recording. */

export class CharlieAudio {
  private ctx: AudioContext | null = null;
  private muted = false;
  private moonTimer: number | null = null;
  private rain: AudioBufferSourceNode | null = null;
  private bedTimer: number | null = null;
  private room: string | null = null;
  private moonOn = false;

  unlock(): void {
    const ctx = this.ensure();
    if (ctx?.state === "suspended") {
      void ctx.resume();
    }
  }

  private ensure(): AudioContext | null {
    if (!this.ctx) {
      const Ctx = window.AudioContext;
      this.ctx = new Ctx();
    }
    return this.ctx;
  }

  private blip(
    ctx: AudioContext,
    freq: number,
    when: number,
    dur: number,
    type: OscillatorType,
    gain: number,
  ): void {
    const osc = ctx.createOscillator();
    const amp = ctx.createGain();
    osc.type = type;
    osc.frequency.setValueAtTime(freq, when);
    amp.gain.setValueAtTime(gain, when);
    amp.gain.exponentialRampToValueAtTime(0.0001, when + dur);
    osc.connect(amp);
    amp.connect(ctx.destination);
    osc.start(when);
    osc.stop(when + dur + 0.02);
  }

  private noise(ctx: AudioContext, when: number, dur: number, gain: number, freq = 800): void {
    const length = Math.floor(ctx.sampleRate * dur);
    const buffer = ctx.createBuffer(1, Math.max(1, length), ctx.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < data.length; i += 1) {
      data[i] = Math.random() * 2 - 1;
    }
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    const filter = ctx.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.value = freq;
    const amp = ctx.createGain();
    amp.gain.setValueAtTime(gain, when);
    amp.gain.exponentialRampToValueAtTime(0.0001, when + dur);
    src.connect(filter);
    filter.connect(amp);
    amp.connect(ctx.destination);
    src.start(when);
    src.stop(when + dur);
  }

  sfx(name: string): void {
    if (this.muted) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    const t = ctx.currentTime + 0.02;
    if (name === "door") {
      this.noise(ctx, t, 0.18, 0.08, 180);
      this.blip(ctx, 140, t, 0.12, "square", 0.04);
    } else if (name === "coin") {
      this.blip(ctx, 988, t, 0.08, "square", 0.05);
      this.blip(ctx, 1318, t + 0.07, 0.1, "square", 0.04);
    } else if (name === "spray") {
      this.noise(ctx, t, 0.35, 0.06, 2400);
    } else if (name === "drink") {
      this.blip(ctx, 220, t, 0.2, "triangle", 0.06);
      this.noise(ctx, t, 0.15, 0.03, 400);
    } else if (name === "score") {
      [523, 659, 784].forEach((freq, i) => this.blip(ctx, freq, t + i * 0.08, 0.12, "square", 0.04));
    } else if (name === "thunder") {
      this.noise(ctx, t, 0.7, 0.12, 90);
      this.blip(ctx, 55, t, 0.5, "sawtooth", 0.03);
    } else if (name === "moon") {
      this.startMoon();
    } else if (name === "step") {
      this.noise(ctx, t, 0.04, 0.02, 200);
    }
  }

  private startMoon(): void {
    this.stopMoon();
    if (this.muted) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    this.moonOn = true;
    const beat = 0.42;
    const melody = [
      349.23, 440, 523.25, 523.25, 587.33, 523.25, 440, 392, 349.23, 392, 440, 523.25, 698.46, 659.25,
      587.33, 523.25, 440, 349.23, 392, 440, 466.16, 440, 392, 349.23,
    ];
    const bass = [174.61, 130.81, 174.61, 130.81, 116.54, 174.61, 130.81, 174.61];
    const start = ctx.currentTime + 0.05;
    melody.forEach((freq, i) => this.blip(ctx, freq, start + i * beat, beat * 0.88, "triangle", 0.05));
    bass.forEach((freq, i) => this.blip(ctx, freq, start + i * beat * 3, beat * 2.2, "square", 0.025));
    this.moonTimer = window.setTimeout(() => {
      if (this.moonOn) {
        this.startMoon();
      }
    }, melody.length * beat * 1000);
  }

  private stopMoon(): void {
    this.moonOn = false;
    if (this.moonTimer !== null) {
      window.clearTimeout(this.moonTimer);
    }
    this.moonTimer = null;
  }

  private stopRain(): void {
    try {
      this.rain?.stop();
    } catch {
      /* already stopped */
    }
    this.rain = null;
  }

  private startRain(): void {
    if (this.rain || this.muted) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    const length = ctx.sampleRate * 2;
    const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < length; i += 1) {
      data[i] = Math.random() * 2 - 1;
    }
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.loop = true;
    const filter = ctx.createBiquadFilter();
    filter.type = "highpass";
    filter.frequency.value = 1200;
    const amp = ctx.createGain();
    amp.gain.value = 0.03;
    src.connect(filter);
    filter.connect(amp);
    amp.connect(ctx.destination);
    src.start();
    this.rain = src;
  }

  private stopBed(): void {
    if (this.bedTimer !== null) {
      window.clearTimeout(this.bedTimer);
    }
    this.bedTimer = null;
  }

  private barBed(): void {
    if (this.muted || this.moonOn) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    const t = ctx.currentTime + 0.05;
    const chords = [
      [174.61, 220, 261.63],
      [196, 246.94, 293.66],
      [164.81, 207.65, 246.94],
      [146.83, 174.61, 220],
    ];
    chords.forEach((chord, i) => {
      for (const freq of chord) {
        this.blip(ctx, freq, t + i * 1.4, 1.2, "triangle", 0.012);
      }
    });
    this.bedTimer = window.setTimeout(() => this.barBed(), 5600);
  }

  private clubBed(): void {
    if (this.muted) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    const t = ctx.currentTime + 0.05;
    this.blip(ctx, 55, t, 0.45, "square", 0.03);
    this.blip(ctx, 82.41, t + 0.5, 0.4, "triangle", 0.02);
    this.bedTimer = window.setTimeout(() => this.clubBed(), 1000);
  }

  private alleyBed(): void {
    if (this.muted) {
      return;
    }
    const ctx = this.ensure();
    if (!ctx) {
      return;
    }
    const t = ctx.currentTime + 0.05;
    this.blip(ctx, 73.42, t, 1.6, "square", 0.02);
    this.blip(ctx, 110, t + 1.6, 1.4, "triangle", 0.015);
    this.bedTimer = window.setTimeout(() => this.alleyBed(), 3200);
  }

  setRoom(room: string | null): void {
    this.room = room;
    this.stopBed();
    if (this.muted || !room) {
      this.stopRain();
      this.stopMoon();
      return;
    }
    if (room === "alley" || room === "rooftop" || room === "street") {
      this.stopMoon();
      this.startRain();
      this.alleyBed();
      return;
    }
    if (room === "club") {
      this.stopMoon();
      this.stopRain();
      this.clubBed();
      return;
    }
    this.stopRain();
    if (room !== "bar") {
      this.stopMoon();
    }
    if (room === "bar" && !this.moonOn) {
      this.barBed();
    }
  }
}
