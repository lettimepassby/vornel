<script lang="ts">
  // SSH 终端底部的服务器资源栏：每 3 秒经已有连接读取一次 /proc（只在当前会话可见时读取）
  import * as api from '../lib/api';
  import type { ServerStats } from '../lib/api';

  let { sessionId, active, onunsupported }: { sessionId: string; active: boolean; onunsupported: () => void } = $props();

  let cur = $state<ServerStats | null>(null);
  let cpu = $state<number | null>(null);
  let rx = $state(0);
  let tx = $state(0);
  let prev: { s: ServerStats; t: number } | null = null;

  async function sample() {
    if (document.visibilityState !== 'visible') return;
    try {
      const s = await api.sessionStats(sessionId);
      if (!s) {
        onunsupported();
        return;
      }
      const t = performance.now();
      if (prev) {
        const dt = (t - prev.t) / 1000;
        const total = s.cpuTotal - prev.s.cpuTotal;
        const idle = s.cpuIdle - prev.s.cpuIdle;
        cpu = total > 0 ? Math.max(0, Math.min(100, (1 - idle / total) * 100)) : cpu;
        rx = Math.max(0, (s.netRx - prev.s.netRx) / dt);
        tx = Math.max(0, (s.netTx - prev.s.netTx) / dt);
      }
      prev = { s, t };
      cur = s;
    } catch {
      // 会话刚断开等情况：下一次再试
    }
  }

  $effect(() => {
    if (!active) return;
    void sample();
    const timer = setInterval(() => void sample(), 3000);
    return () => clearInterval(timer);
  });

  const size = (b: number) => {
    const u = ['B', 'K', 'M', 'G', 'T'];
    let i = 0;
    while (b >= 1024 && i < u.length - 1) {
      b /= 1024;
      i++;
    }
    return `${b >= 100 || i === 0 ? b.toFixed(0) : b.toFixed(1)}${u[i]}`;
  };
  const pct = (used: number, total: number) => (total ? (used / total) * 100 : 0);
  /** 负载高时变色：≥70% 警告，≥90% 危险 */
  const level = (p: number) => (p >= 90 ? 'err' : p >= 70 ? 'warn' : '');
  const uptime = (s: number) => {
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    return d ? `${d}天${h}时` : `${h}时${Math.floor((s % 3600) / 60)}分`;
  };

  const mem = $derived(cur ? pct(cur.memTotal - cur.memAvailable, cur.memTotal) : 0);
  const disk = $derived(cur ? pct(cur.diskUsed, cur.diskTotal) : 0);
  const swap = $derived(cur && cur.swapTotal ? pct(cur.swapTotal - cur.swapFree, cur.swapTotal) : null);
  const loadPct = $derived(cur ? (cur.load[0] / cur.cores) * 100 : 0);
</script>

{#if cur}
  <div class="stats mono" role="status" aria-label="服务器资源">
    <span class="it {level(cpu ?? 0)}" title="CPU 使用率（{cur.cores} 核）">
      <b>CPU</b>{cpu === null ? '…' : `${cpu.toFixed(0)}%`}
      <i class="bar"><i style:width="{cpu ?? 0}%"></i></i>
    </span>
    <span class="it {level(mem)}" title="内存：已用 {size(cur.memTotal - cur.memAvailable)} / {size(cur.memTotal)}">
      <b>内存</b>{size(cur.memTotal - cur.memAvailable)}/{size(cur.memTotal)}
      <i class="bar"><i style:width="{mem}%"></i></i>
    </span>
    {#if swap !== null}
      <span class="it {level(swap)}" title="Swap：已用 {size(cur.swapTotal - cur.swapFree)} / {size(cur.swapTotal)}"><b>Swap</b>{swap.toFixed(0)}%</span>
    {/if}
    {#if cur.diskTotal}
      <span class="it {level(disk)}" title="根分区：已用 {size(cur.diskUsed)} / {size(cur.diskTotal)}"><b>磁盘</b>{disk.toFixed(0)}%</span>
    {/if}
    <span class="it {level(loadPct)}" title="1 / 5 / 15 分钟平均负载（{cur.cores} 核）"><b>负载</b>{cur.load.map((l) => l.toFixed(2)).join(' ')}</span>
    <span class="it" title="网络（不含回环）"><b>↓</b>{size(rx)}/s <b>↑</b>{size(tx)}/s</span>
    <span class="sp"></span>
    <span class="it" title="运行时长"><b>运行</b>{uptime(cur.uptimeSecs)}</span>
  </div>
{/if}

<style>
  .stats {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 24px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 12px;
    border-top: 1px solid color-mix(in srgb, currentColor 12%, transparent);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    opacity: 0.85;
    animation: stats-in 240ms ease-out;
  }
  @keyframes stats-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  .it {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    transition: color 300ms ease-out;
  }
  b {
    font-weight: 500;
    opacity: 0.6;
  }
  .bar {
    width: 34px;
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, currentColor 15%, transparent);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: currentColor;
    border-radius: 2px;
    transition: width 600ms ease-out;
  }
  .warn {
    color: var(--warn);
  }
  .err {
    color: var(--err);
  }
  .sp {
    flex: 1;
  }
</style>
