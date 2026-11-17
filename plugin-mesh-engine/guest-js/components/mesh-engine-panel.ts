/**
 * <mesh-engine-panel> — статус/установка/обновление движка trellis.cpp (зеркало
 * UX <image-engine-panel>: install, «Проверить обновление», «Обновить», бейдж
 * обновления, прогресс downloader:progress).
 */
import { STYLE, toast, progressText } from './ui-shared'
import {
  getMeshEngineStatus,
  installMeshEngine,
  setMeshEngineDir,
  getRigEngineStatus,
  installRigEngine,
  setRigEngineDir,
  type MeshEngineStatus,
  type RigEngineStatus,
} from '../index'
import { checkMeshUpdate, onUpdateMeshState } from '../updates'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'

const QUANT_HINTS: Record<string, string> = {
  q4: 'q4 — ~6 ГБ (лёгкий, заметная потеря качества)',
  q8: 'q8 — ~9.5 ГБ (почти без потерь; рекомендуемый)',
  f16: 'f16 — ~16.5 ГБ (полный)',
}

const BACKEND_HINTS: Record<string, string> = {
  auto: 'Авто: NVIDIA → CUDA (cc<7.5 → CUDA12-legacy), иначе Vulkan.',
  vulkan: 'Vulkan — универсальный, ~14 МБ. Работает на любом GPU.',
  cuda: 'CUDA — NVIDIA (715 МБ). Нужен современный драйвер.',
  cuda12: 'CUDA12 — legacy-рантайм для старых NVIDIA (cc < 7.5).',
  rocm: 'ROCm — AMD (190 МБ). Требует TheRock ROCm runtime в PATH.',
}

export class MeshEnginePanel extends HTMLElement {
  private root!: ShadowRoot
  private unlisten?: () => void
  private unsubUpdate?: () => void

  connectedCallback() {
    if (this.root) return
    this.root = this.attachShadow({ mode: 'open' })
    this.root.innerHTML = `
      <style>${STYLE}</style>
      <div>
        <div class="row">
          <label>Статус:</label><span id="status" class="hint">Проверка…</span><span id="updateBadge" style="display:none; margin-left:8px; color:#e6a23c;">🔄</span>
        </div>
        <div class="row"><label>Путь:</label><span id="path" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div class="row"><label>Веса:</label><span id="weights" class="hint" style="flex:1;"></span></div>
        <div class="row" style="margin-top:8px;">
          <label>Квант:</label>
          <select id="quant" style="flex:1; min-width:220px;"></select>
        </div>
        <div class="row">
          <label>Бекенд:</label>
          <select id="backend" style="flex:1; min-width:220px;"></select>
        </div>
        <div id="hint" class="hint"></div>
        <div class="row" style="margin-top:10px;">
          <button id="install" class="primary">Установить</button>
          <button id="checkUpdate" class="secondary" style="display:none;">Проверить обновление</button>
          <button id="installUpdate" class="primary" style="display:none;">Обновить</button>
          <button id="setDir" class="secondary">Изменить путь</button>
        </div>
        <div id="progress" class="progress-container">
          <div class="progress-status" id="progressStatus">0 MB / 0 MB</div>
          <div class="progress-track"><div class="progress-bar" id="progressBar"></div></div>
        </div>
        <hr style="border:none; border-top:1px solid rgba(128,128,128,.35); margin:14px 0 10px;" />
        <div class="row"><label><b>Риггинг</b> (скелет + веса скиннинга для GLB):</label><span id="rigStatus" class="hint">Проверка…</span></div>
        <div class="row"><label>Путь:</label><span id="rigPath" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div class="row"><label>Веса:</label><span id="rigWeights" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div id="rigHint" class="hint"></div>
        <div class="row" style="margin-top:8px;">
          <button id="rigInstall" class="primary">Скачать веса риггинга (1.25 ГБ)</button>
          <button id="rigSetDir" class="secondary">Изменить путь</button>
        </div>
        <div id="rigProgress" class="progress-container">
          <div class="progress-status" id="rigProgressStatus">0 MB / 0 MB</div>
          <div class="progress-track"><div class="progress-bar" id="rigProgressBar"></div></div>
        </div>
      </div>`

    this.root.getElementById('backend')!.addEventListener('change', () => this.renderHint())
    this.root.getElementById('install')!.addEventListener('click', () => void this.onInstall())
    this.root.getElementById('checkUpdate')!.addEventListener('click', () => void this.onCheckUpdate())
    this.root.getElementById('installUpdate')!.addEventListener('click', () => void this.onInstall())
    this.root.getElementById('setDir')!.addEventListener('click', () => void this.onSetDir())
    this.root.getElementById('rigInstall')!.addEventListener('click', () => void this.onRigInstall())
    this.root.getElementById('rigSetDir')!.addEventListener('click', () => void this.onRigSetDir())

    this.unsubUpdate = onUpdateMeshState((s) => this.renderUpdateState(s))
    void this.refresh()
    void listen<{ downloaded: number; total: number; kind?: string; status?: string }>(
      'downloader:progress',
      (e) => {
        const p = e.payload
        if (p.kind === 'mesh-engine') {
          this.setProgress(p.downloaded, p.total)
          if (p.status === 'done' || p.status === 'error') {
            setTimeout(() => this.showProgress(false), 400)
          }
        } else if (p.kind === 'rig-engine') {
          this.rigSetProgress(p.downloaded, p.total)
          if (p.status === 'done' || p.status === 'error') {
            setTimeout(() => this.showRigProgress(false), 400)
          }
        }
      },
    ).then((u) => { this.unlisten = u }).catch(() => {})
  }

  disconnectedCallback() {
    this.unlisten?.()
    this.unsubUpdate?.()
  }

  private setStatus(s: string) {
    const el = this.root.getElementById('status')
    if (el) el.textContent = s
  }

  private setProgress(downloaded: number, total: number) {
    const bar = this.root.getElementById('progressBar') as HTMLElement
    bar.style.width = `${total > 0 ? (downloaded / total) * 100 : 0}%`
    const st = this.root.getElementById('progressStatus')!
    st.textContent = progressText(downloaded, total)
  }

  private showProgress(on: boolean) {
    this.root.getElementById('progress')!.style.display = on ? 'block' : 'none'
  }

  private weightsSummary(st: MeshEngineStatus): string {
    if (!st.weights.length) return '—'
    return st.weights
      .map((w) => `${w.quant}: ${w.ggufCount} файлов${w.pixal3dFiles ? `, pixal3d: ${w.pixal3dFiles}` : ''}`)
      .join(', ')
  }

  private renderHint() {
    const backend = (this.root.getElementById('backend') as HTMLSelectElement).value
    const hint = this.root.getElementById('hint')!
    hint.textContent = BACKEND_HINTS[backend] ?? ''
  }

  // ── Секция «Риггинг» (skin-tokens.cpp): статус + докачка 3 весов F16 ──

  private rigSetProgress(downloaded: number, total: number) {
    const bar = this.root.getElementById('rigProgressBar') as HTMLElement
    bar.style.width = `${total > 0 ? (downloaded / total) * 100 : 0}%`
    const st = this.root.getElementById('rigProgressStatus')!
    st.textContent = progressText(downloaded, total)
  }

  private showRigProgress(on: boolean) {
    this.root.getElementById('rigProgress')!.style.display = on ? 'block' : 'none'
  }

  private renderRig(st: RigEngineStatus) {
    this.root.getElementById('rigPath')!.textContent = st.dir || '—'
    this.root.getElementById('rigWeights')!.textContent = st.weightsDir
      ? `${st.weightsPresent}/${st.weightsTotal} (F16) — ${st.weightsDir}`
      : '—'
    const status = this.root.getElementById('rigStatus')!
    const hint = this.root.getElementById('rigHint')!
    if (st.ready) {
      status.textContent = 'Готов к ригу'
      hint.textContent = ''
    } else {
      const missing: string[] = []
      if (!st.exePresent) missing.push('skintokens-cli.exe')
      if (!st.dllsPresent) missing.push('рантайм-DLL движка')
      if (st.weightsPresent < st.weightsTotal) missing.push(`веса (${st.weightsPresent}/${st.weightsTotal})`)
      status.textContent = `Не готов: ${missing.join(', ')}`
      // Честный нюанс (PRP этап 3): пребилдов CLI upstream нет — exe/DLL
      // кладутся из локальной сборки, установщик качает только веса.
      hint.textContent = st.exePresent
        ? ''
        : 'CLI риггинга не скачивается панелью (пребилдов upstream нет) — exe/DLL кладутся в папку вручную из локальной сборки skin-tokens.cpp.'
    }
    const btn = this.root.getElementById('rigInstall') as HTMLButtonElement
    btn.textContent = st.weightsPresent === st.weightsTotal
      ? 'Докачать веса риггинга'
      : 'Скачать веса риггинга (1.25 ГБ)'
  }

  private async refreshRig() {
    try {
      this.renderRig(await getRigEngineStatus())
    } catch (e) {
      this.root.getElementById('rigStatus')!.textContent = `Ошибка: ${e}`
    }
  }

  private async onRigInstall() {
    const btn = this.root.getElementById('rigInstall') as HTMLButtonElement
    btn.disabled = true
    this.showRigProgress(true)
    this.root.getElementById('rigProgressStatus')!.textContent = 'Подготовка…'
    try {
      const st = await installRigEngine()
      this.renderRig(st)
      toast('Веса риггинга скачаны!')
    } catch (e) {
      toast(`Ошибка скачивания весов риггинга: ${e}`, 'error')
    } finally {
      btn.disabled = false
      this.showRigProgress(false)
      void this.refreshRig()
    }
  }

  private async onRigSetDir() {
    try {
      const sel = await openDialog({ directory: true, title: 'Папка движка риггинга' })
      if (!sel) return
      const path = Array.isArray(sel) ? sel[0] : sel
      if (!path) return
      const st = await setRigEngineDir(path)
      this.renderRig(st)
      toast('Путь риггинга изменён.')
    } catch (e) {
      toast(`Ошибка изменения пути риггинга: ${e}`, 'error')
    }
  }

  private async refresh() {
    let st: MeshEngineStatus
    try {
      st = await getMeshEngineStatus()
    } catch (e) {
      this.setStatus(`Ошибка: ${e}`)
      return
    }
    this.setStatus(st.installed ? 'Движок установлен' : 'Движок не установлен')
    this.root.getElementById('path')!.textContent = st.dir || '—'
    this.root.getElementById('weights')!.textContent = this.weightsSummary(st)

    const quant = this.root.getElementById('quant') as HTMLSelectElement
    quant.innerHTML = ''
    for (const q of ['q8', 'q4', 'f16']) {
      const o = document.createElement('option')
      o.value = q
      o.textContent = QUANT_HINTS[q] ?? q
      quant.appendChild(o)
    }
    const backend = this.root.getElementById('backend') as HTMLSelectElement
    backend.innerHTML = ''
    for (const b of ['auto', 'vulkan', 'cuda', 'cuda12', 'rocm']) {
      const o = document.createElement('option')
      o.value = b
      o.textContent = b === 'auto' ? 'Авто (рекомендуется)' : b
      backend.appendChild(o)
    }
    this.renderHint()
    const installBtn = this.root.getElementById('install') as HTMLButtonElement
    installBtn.textContent = st.installed ? 'Докачать / обновить' : 'Установить'
    const checkBtn = this.root.getElementById('checkUpdate') as HTMLButtonElement
    checkBtn.style.display = st.installed ? '' : 'none'
    void this.refreshRig()
  }

  private renderUpdateState(s: { hasUpdate: boolean; tag?: string }) {
    const badge = this.root.getElementById('updateBadge')!
    badge.style.display = s.hasUpdate ? '' : 'none'
    const updBtn = this.root.getElementById('installUpdate') as HTMLButtonElement
    updBtn.style.display = s.hasUpdate ? '' : 'none'
  }

  private async onCheckUpdate() {
    const btn = this.root.getElementById('checkUpdate') as HTMLButtonElement
    btn.disabled = true
    try {
      const s = await checkMeshUpdate()
      if (s.hasUpdate) toast(`Доступно обновление 3D-движка: ${s.tag}`)
      else toast('Движок 3D-генерации актуален')
    } catch (e) {
      toast(`Не удалось проверить обновления: ${e}`, 'error')
    } finally {
      btn.disabled = false
    }
  }

  private async onInstall() {
    const btn = this.root.getElementById('install') as HTMLButtonElement
    btn.disabled = true
    this.showProgress(true)
    this.root.getElementById('progressStatus')!.textContent = 'Подготовка…'
    const quant = (this.root.getElementById('quant') as HTMLSelectElement).value
    const backend = (this.root.getElementById('backend') as HTMLSelectElement).value
    try {
      const st = await installMeshEngine({ backend, quant })
      this.setStatus(st.installed ? 'Движок установлен' : 'Движок не установлен')
      this.root.getElementById('weights')!.textContent = this.weightsSummary(st)
      await checkMeshUpdate().catch(() => {}) // перечитать стейт обновления (бейдж гаснет)
      toast('Движок 3D-генерации установлен!')
    } catch (e) {
      toast(`Ошибка установки движка: ${e}`, 'error')
    } finally {
      btn.disabled = false
      this.showProgress(false)
      await this.refresh()
    }
  }

  private async onSetDir() {
    try {
      const sel = await openDialog({ directory: true, title: 'Папка движка 3D-генерации' })
      if (!sel) return
      const path = Array.isArray(sel) ? sel[0] : sel
      if (!path) return
      const st = await setMeshEngineDir(path)
      this.root.getElementById('path')!.textContent = st.dir || '—'
      this.setStatus(st.installed ? 'Движок установлен' : 'Движок не установлен')
      this.root.getElementById('weights')!.textContent = this.weightsSummary(st)
      toast('Путь движка изменён.')
    } catch (e) {
      toast(`Ошибка изменения пути: ${e}`, 'error')
    }
  }
}

if (!customElements.get('mesh-engine-panel')) {
  customElements.define('mesh-engine-panel', MeshEnginePanel)
}
