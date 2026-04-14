import { copyFileSync, mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const websiteRoot = resolve(__dirname, '..')
const repoRoot = resolve(websiteRoot, '..')

const profileFlagIndex = process.argv.indexOf('--profile')
const profile =
  profileFlagIndex >= 0 && process.argv[profileFlagIndex + 1]
    ? process.argv[profileFlagIndex + 1]
    : 'release'

const cargoArgs = ['build', '-p', 'mustard-wasm', '--target', 'wasm32-unknown-unknown']
if (profile === 'release') {
  cargoArgs.push('--release')
}

const cargoProfileDir = profile === 'release' ? 'release' : 'debug'

execFileSync('cargo', cargoArgs, {
  cwd: repoRoot,
  stdio: 'inherit',
})

const wasmSource = resolve(
  repoRoot,
  'target',
  'wasm32-unknown-unknown',
  cargoProfileDir,
  'mustard_wasm.wasm',
)
const wasmTarget = resolve(websiteRoot, 'public', 'mustard-playground.wasm')

mkdirSync(dirname(wasmTarget), { recursive: true })
copyFileSync(wasmSource, wasmTarget)
