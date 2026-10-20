/** Override at build time, e.g. self-hosted copy of docs/compliance.md */
export const complianceUrl =
  import.meta.env.VITE_COMPLIANCE_URL ||
  'https://github.com/danmo-ai/danmo-router/blob/main/docs/compliance.md'

export function ComplianceAuthNotice() {
  return (
    <p className="legal-notice muted">
      本实例由部署方独立运营。须使用合法授权的上游凭据，不得用于规避上游协议的中转或转售。详见{' '}
      <a href={complianceUrl} target="_blank" rel="noopener noreferrer">
        合规与可接受使用说明
      </a>
      。
    </p>
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
