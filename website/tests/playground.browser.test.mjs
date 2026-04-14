import test from 'node:test'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'

const __dirname = dirname(fileURLToPath(import.meta.url))
const websiteRoot = resolve(__dirname, '..')
const baseUrl = 'http://127.0.0.1:4173'

function npmCommand() {
  return process.platform === 'win32' ? 'npm.cmd' : 'npm'
}

function outputSection(page, title) {
  if (title === 'Mustard Output') {
    return page.getByTestId('playground-output-mustard')
  }
  if (title === 'Vanilla Output') {
    return page.getByTestId('playground-output-vanilla')
  }
  throw new Error(`unknown output section: ${title}`)
}

async function waitForServer(url, timeoutMs = 20_000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const response = await fetch(url)
      if (response.ok) {
        return
      }
    } catch {
      // ignore
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw new Error(`server did not become ready: ${url}`)
}

test(
  'playground loads, switches scenarios, runs successfully, and renders failure states',
  { concurrency: false },
  async () => {
  const server = spawn(
    npmCommand(),
    ['run', 'dev', '--', '--host', '127.0.0.1', '--port', '4173', '--strictPort'],
    {
      cwd: websiteRoot,
      stdio: 'pipe',
      env: process.env,
    },
  )

  const browser = await chromium.launch()

  try {
    await waitForServer(baseUrl)
    const page = await browser.newPage()
    await page.goto(baseUrl)
    const mustardOutput = outputSection(page, 'Mustard Output')
    const vanillaOutput = outputSection(page, 'Vanilla Output')

    await assert.doesNotReject(async () => {
      await page.getByRole('heading', { name: 'Taste the MustardScript' }).waitFor()
    })

    await page.getByRole('button', { name: 'Policy Check' }).click()
    await page.getByText('Compare deterministic policy checks').waitFor()

    await page.getByRole('button', { name: 'Run Comparison' }).click()
    await mustardOutput.getByText('matches expected').waitFor()
    await mustardOutput.getByText('"approved": false').waitFor()
    await mustardOutput.getByText('Capability Trace').waitFor()
    await vanillaOutput.getByText('matches expected').waitFor()
    await vanillaOutput.getByText('"approved": false').waitFor()

    const vanillaEditor = page.getByLabel('Vanilla JavaScript')
    await vanillaEditor.fill('throw new Error("browser failure")')
    await page.getByRole('button', { name: 'Run Comparison' }).click()
    await vanillaOutput.getByText('browser failure').waitFor()
    await mustardOutput.getByText('matches expected').waitFor()
  } finally {
    await browser.close()
    server.kill('SIGTERM')
  }
  },
)

test('playground still runs when crypto.randomUUID is unavailable', { concurrency: false }, async () => {
  const server = spawn(
    npmCommand(),
    ['run', 'dev', '--', '--host', '127.0.0.1', '--port', '4173', '--strictPort'],
    {
      cwd: websiteRoot,
      stdio: 'pipe',
      env: process.env,
    },
  )

  const browser = await chromium.launch()

  try {
    await waitForServer(baseUrl)
    const page = await browser.newPage()
    await page.addInitScript(() => {
      Object.defineProperty(globalThis.crypto, 'randomUUID', {
        value: undefined,
        configurable: true,
      })
    })
    await page.goto(baseUrl)

    const vanillaOutput = outputSection(page, 'Vanilla Output')
    await page.getByRole('button', { name: 'Run Comparison' }).click()
    await vanillaOutput.getByText('matches expected').waitFor()
    await vanillaOutput.getByText('"quoteId": "quote-acct_91-25"').waitFor()
  } finally {
    await browser.close()
    server.kill('SIGTERM')
  }
})
