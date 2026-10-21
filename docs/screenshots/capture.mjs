import { chromium } from 'playwright'

const out = new URL('.', import.meta.url).pathname

async function shot(page, name) {
  await page.waitForTimeout(400)
  await page.screenshot({ path: `${out}${name}.png`, clip: { x: 0, y: 0, width: 1440, height: 900 } })
}

const browser = await chromium.launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })

const admin = await context.newPage()
await admin.goto('http://127.0.0.1:5173/login', { waitUntil: 'networkidle' })
await admin.goto('http://127.0.0.1:5173/', { waitUntil: 'networkidle' })
if (await admin.locator('input').count()) {
  await admin.locator('input').nth(0).fill('admin')
  await admin.locator('input').nth(1).fill('admin123')
  await admin.getByRole('button', { name: '登录' }).click()
  await admin.waitForURL('**/', { timeout: 10000 }).catch(() => {})
  await admin.waitForTimeout(600)
}
const adminPages = [
  ['/', 'admin-overview'],
  ['/channels', 'admin-channels'],
  ['/channels/new', 'admin-channel-form'],
  ['/users', 'admin-users'],
  ['/prices', 'admin-prices'],
  ['/redemptions', 'admin-redemptions'],
  ['/logs', 'admin-logs'],
]
for (const [path, name] of adminPages) {
  await admin.goto(`http://127.0.0.1:5173${path}`, { waitUntil: 'networkidle' })
  await shot(admin, name)
}

const userContext = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })
const user = await userContext.newPage()
await user.goto('http://127.0.0.1:5174/', { waitUntil: 'networkidle' })
await user.locator('input').nth(0).fill('demo')
await user.locator('input').nth(1).fill('demo1234')
await user.locator('button[type="submit"]').click()
await user.waitForTimeout(800)
const userPages = [
  ['/', 'user-overview'],
  ['/tokens', 'user-tokens'],
  ['/models', 'user-models'],
  ['/logs', 'user-logs'],
]
for (const [path, name] of userPages) {
  await user.goto(`http://127.0.0.1:5174${path}`, { waitUntil: 'networkidle' })
  await shot(user, name)
}

await browser.close()
console.log('captured')
