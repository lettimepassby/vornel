// 主机页的“回到全部主机”信号：已经在主机页时再点侧边栏的“主机”，清空搜索、退出分组、回到顶部。
export const hostsNav = $state({ resetCount: 0 });

export function resetHostsView() {
  hostsNav.resetCount += 1;
}
