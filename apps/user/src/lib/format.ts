export function fmtNum(n: number) {
  return new Intl.NumberFormat('zh-CN').format(n)
}
