// Web Component <audio-gen-panel>: компактная панель генерации SFX/музыки.
// Shadow DOM + var(--token) темы хоста (PLUGIN_STANDARD §6). Vanilla-каналу
// панель не обязательна — defs вызывают defineAudioGenPanel() явно.
import { cancel, generate, listModels, onEvent, openOutputDir, status } from './index'
import type { ModelEntry, StatusSnapshot } from './index'

export function defineAudioGenPanel(tag = 'audio-gen-panel'): void {
  if (customElements.get(tag)) return

  class AudioGenPanel extends HTMLElement {
    private root: ShadowRoot
    private models: ModelEntry[] = []
    private unlisten: (() => void) | null = null
    private currentJobId: string | null = null

    constructor() {
      super()
      this.root = this.attachShadow({ mode: 'open' })
      this.root.innerHTML = `
        <style>
          :host { display: block; font-family: inherit; color: var(--ag-fg, inherit); }
          .card { border: 1px solid var(--ag-border, #ccc3); border-radius: 10px; padding: 12px; display: flex; flex-direction: column; gap: 8px; }
          label { display: flex; flex-direction: column; gap: 3px; font-size: 12px; opacity: 0.85; }
          select, textarea, input {
            font: inherit; color: inherit; background: var(--ag-bg2, transparent);
            border: 1px solid var(--ag-border, #ccc3); border-radius: 6px; padding: 5px 7px;
          }
          textarea { min-height: 56px; resize: vertical; }
          .row { display: flex; gap: 8px; }
          .row > label { flex: 1; }
          button {
            font: inherit; cursor: pointer; border-radius: 6px; padding: 6px 12px;
            border: 1px solid var(--ag-border, #ccc3); background: var(--ag-accent, #4a7dff22); color: inherit;
          }
          button:disabled { opacity: 0.5; cursor: default; }
          .status { font-size: 12px; opacity: 0.8; min-height: 1.2em; white-space: pre-wrap; }
          .lic { font-size: 11px; opacity: 0.6; }
        </style>
        <div class="card">
          <label>Модель <select id="model"></select></label>
          <div class="lic" id="lic"></div>
          <label>Промпт (что за звук/музыка) <textarea id="prompt"></textarea></label>
          <div class="row">
            <label>Длина, с <input id="dur" type="number" min="1" max="380" value="4" /></label>
            <label>Имя файла <input id="name" placeholder="авто" /></label>
            <label>Луп (кроссфейд) <input id="loop" type="checkbox" /></label>
          </div>
          <div class="row">
            <button id="go">Сгенерировать</button>
            <button id="cancel" disabled>Отменить</button>
            <button id="open">Папка вывода</button>
          </div>
          <div class="status" id="st">готов</div>
        </div>
      `
      const $ = (id: string) => this.root.getElementById(id) as HTMLElement & { onclick?: () => void }
      $('go').onclick = () => void this.submit()
      $('cancel').onclick = () => void this.cancelCurrent()
      $('open').onclick = () => void openOutputDir()
      this.el('model').addEventListener('change', () => this.updateLicense())
      void this.refreshModels()
      void this.subscribe()
    }

    disconnectedCallback(): void {
      this.unlisten?.()
      this.unlisten = null
    }

    /** Элемент панели по id с гарантией ненулевости (strict DOM). */
    private el<T extends HTMLElement = HTMLElement>(id: string): T {
      const e = this.root.getElementById(id) as T | null
      if (!e) throw new Error(`audio-gen-panel: нет элемента #${id}`)
      return e
    }

    private async refreshModels(): Promise<void> {
      try {
        this.models = await listModels()
        const sel = this.el<HTMLSelectElement>('model')
        sel.innerHTML = ''
        for (const m of this.models) {
          const opt = document.createElement('option')
          opt.value = m.id
          opt.textContent = `${m.name}${m.installed ? '' : ' (не установлена)'}`
          sel.appendChild(opt)
        }
        this.updateLicense()
      } catch (e) {
        this.say(`ошибка списка моделей: ${String(e)}`)
      }
    }

    private updateLicense(): void {
      const sel = this.el<HTMLSelectElement>('model')
      const m = this.models.find((x) => x.id === sel.value)
      this.el('lic').textContent = m
        ? `${m.license} · ${m.sample_rate} Гц · до ${m.max_duration_secs} с`
        : ''
    }

    private async submit(): Promise<void> {
      const sel = this.el<HTMLSelectElement>('model')
      const prompt = this.el<HTMLTextAreaElement>('prompt').value.trim()
      if (!prompt) return this.say('введите промпт')
      const model = this.models.find((m) => m.id === sel.value)
      const durEl = this.el<HTMLInputElement>('dur')
      const nameEl = this.el<HTMLInputElement>('name')
      const loopEl = this.el<HTMLInputElement>('loop')
      try {
        this.currentJobId = await generate({
          modelId: sel.value,
          prompt,
          durationSecs: durEl.valueAsNumber > 0 ? durEl.valueAsNumber : undefined,
          steps: model && model.default_steps !== 100 ? model.default_steps : undefined,
          makeLoop: loopEl.checked,
          name: nameEl.value.trim() || undefined,
        })
        this.say(`задача ${this.currentJobId} запущена…`)
      } catch (e) {
        this.say(`ошибка запуска: ${String(e)}`)
      }
    }

    private async cancelCurrent(): Promise<void> {
      if (!this.currentJobId) return
      try {
        await cancel(this.currentJobId)
        this.say('отменяю…')
      } catch (e) {
        this.say(`ошибка отмены: ${String(e)}`)
      }
    }

    private async subscribe(): Promise<void> {
      this.unlisten = await onEvent((e) => {
        if (e.jobId !== this.currentJobId) return
        this.say(`[${e.status}] ${e.progress}%`)
        this.setBusy(e.status === 'running' || e.status === 'queued')
        if (e.status === 'done') void this.showResult(e.jobId)
        if (e.status === 'failed' || e.status === 'cancelled') {
          this.currentJobId = null
          void this.refreshStatusMessage()
        }
      })
    }

    private async showResult(jobId: string): Promise<void> {
      this.currentJobId = null
      const snap: StatusSnapshot = await status()
      const job = snap.jobs.find((j) => j.id === jobId)
      this.say(job?.output_path ? `готово: ${job.output_path}` : 'готово')
    }

    private async refreshStatusMessage(): Promise<void> {
      this.say('готов')
    }

    private setBusy(b: boolean): void {
      this.el<HTMLButtonElement>('go').disabled = b
      this.el<HTMLButtonElement>('cancel').disabled = !b
    }

    private say(text: string): void {
      this.el('st').textContent = text
    }
  }

  customElements.define(tag, AudioGenPanel)
}
