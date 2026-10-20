/** Override at build time, e.g. self-hosted copy of docs/compliance.md */
export const complianceUrl =
  import.meta.env.VITE_COMPLIANCE_URL ||
  'https://github.com/danmo-ai/danmo-router/blob/main/docs/compliance.md'

export function ComplianceAuthNotice() {
  return (
    <p className="legal-notice muted">
      本服务由部署方运营。调用模型须遵守上游条款与当地法律；服务方应使用合法取得的上游授权。详见{' '}
      <a href={complianceUrl} target="_blank" rel="noopener noreferrer">
        合规与可接受使用说明
      </a>
      。
    </p>
  )
}

export function ComplianceSiteFooter() {
  return (
    <footer className="site-legal">
      <span>danmo-router · 部署方须保证上游凭据合法授权</span>
      <ComplianceFooterLink />
    </footer>
  )
}

export function ComplianceFooterLink() {
  return (
    <a
      className="legal-link"
      href={complianceUrl}
      target="_blank"
      rel="noopener noreferrer"
    >
      合规与可接受使用
    </a>
  )
}
