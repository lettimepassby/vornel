<script lang="ts">
  import * as api from '../lib/api';
  import type { KnownHost } from '../lib/api';
  import { data, loadAll } from '../lib/store.svelte';

  let confirmId = $state<string | null>(null);

  async function del(k: KnownHost) {
    if (confirmId !== k.id) {
      confirmId = k.id;
      return;
    }
    await api.remove('known_host', k.id);
    confirmId = null;
    await loadAll();
  }

  const date = (s: number) => new Date(s * 1000).toLocaleDateString();
</script>

<div class="page">
  <div class="page-head">
    <h2>已知主机</h2>
    <span class="n">{data.knownHosts.length}</span>
  </div>
  <p class="page-sub">
    首次连接时记录服务器的公钥指纹；之后如果指纹改变，Vornel 会拒绝连接以防中间人攻击。已知主机会随 Vault 同步到你的其他设备。
  </p>

  {#if data.knownHosts.length}
    <div class="items">
      {#each data.knownHosts as k (k.id)}
        <div class="item">
          <div class="main">
            <b class="mono">{k.host}{k.port !== 22 ? `:${k.port}` : ''} <span class="tag">{k.algorithm}</span></b>
            <small>{k.fingerprint} · {date(k.addedAt)} 记录</small>
          </div>
          <div class="acts">
            <button class="btn danger" type="button" onclick={() => del(k)}>{confirmId === k.id ? '确认删除？' : '删除'}</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="items"><div class="empty-note">还没有记录。连接一台主机后会自动出现在这里。</div></div>
  {/if}
</div>
