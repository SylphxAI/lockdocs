import { createSignal, For, onCleanup, Show, untrack } from "solid-js";
import { ArrowRight, FileCode2, FolderOpen } from "lucide-solid";
import { CopyButton } from "@/components/copy-button";
import type { EventParams } from "@/lib/analytics-events";
import { formats, type Format } from "@/lib/lockfile-formats";
import { samples } from "@/lib/playground-samples";
import { offlineCommand, previewLimit, readPreviewFile } from "@/lib/playground-file";
import { useSiteLanguage } from "@/lib/site-language";

type SourceKind = EventParams<"lockfile_resolved">["source_kind"];

export function StudioInput(props: {
  initialFormat: Format;
  initialInput: string;
  resolve: (format: Format, input: string, source: SourceKind) => void;
}) {
  const { text } = useSiteLanguage();
  const initialFormat = untrack(() => props.initialFormat);
  const initialInput = untrack(() => props.initialInput);
  const [format, setFormat] = createSignal<Format>(initialFormat);
  const [dragging, setDragging] = createSignal(false);
  const [fileStatus, setFileStatus] = createSignal<"oversize" | "unreadable">();
  let editor!: HTMLTextAreaElement;
  let picker!: HTMLInputElement;
  let source: SourceKind = "sample";
  let bundledInput = initialInput;
  let readId = 0;

  function cancelRead() {
    readId++;
    setFileStatus(undefined);
  }
  onCleanup(() => { readId++; });

  function currentSource(): SourceKind {
    return source === "sample" && editor.value !== bundledInput ? "pasted" : source;
  }
  function submit(event: SubmitEvent) {
    event.preventDefault();
    cancelRead();
    props.resolve(format(), editor.value, currentSource());
  }
  function loadSample(next: Format) {
    cancelRead();
    setFormat(next);
    bundledInput = next === initialFormat ? initialInput : samples[next];
    editor.value = bundledInput;
    source = "sample";
    props.resolve(next, editor.value, source);
  }
  function changeFormat(next: Format) {
    // Switch bundled examples immediately, but never replace the visitor’s edits.
    if (currentSource() === "sample") loadSample(next);
    else { cancelRead(); source = currentSource(); setFormat(next); }
  }
  async function openFile(file: File | undefined) {
    if (!file) return;
    cancelRead();
    const id = readId;
    const result = await readPreviewFile(file, format());
    if (id !== readId) return;
    if (result.kind !== "ready") { setFileStatus(result.kind); return; }
    setFormat(result.format);
    editor.value = result.text;
    source = "file";
    props.resolve(result.format, result.text, source);
  }

  return <form class="studio-input" classList={{ "studio-dragging": dragging() }} onSubmit={submit}
    onDragOver={event => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      setDragging(true);
    }}
    onDragLeave={event => {
      if (!(event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget))) setDragging(false);
    }}
    onDrop={event => {
      setDragging(false);
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      void openFile(event.dataTransfer.files[0]);
    }}>
    <div class="panel-title"><FileCode2 size={17}/> <label for="lockfile">{text("Lockfile input", "輸入 lockfile")}</label></div>
    <div class="input-tools">
      <select aria-label={text("Lockfile format", "lockfile 格式")} onChange={event => changeFormat(event.currentTarget.value as Format)}>
        <For each={formats}>{option => <option selected={format() === option}>{option}</option>}</For>
      </select>
      <button type="button" class="button secondary small" onClick={() => picker.click()}><FolderOpen size={15} aria-hidden="true"/> {text("Open file", "開啟檔案")}</button>
      <input type="file" ref={picker} hidden onChange={event => {
        const file = event.currentTarget.files?.[0];
        event.currentTarget.value = "";
        void openFile(file);
      }}/>
      <button type="button" class="button secondary small" onClick={() => loadSample(format())}>{text("Load sample", "載入範例")}</button>
    </div>
    {/* HTML uses the textarea's contents, not a value attribute, for its initial value. */}
    <textarea id="lockfile" ref={editor} onInput={() => { cancelRead(); source = "pasted"; }} spellcheck={false}
      maxlength={previewLimit}>{initialInput}</textarea>
    <div class="inline-actions">
      <button class="button primary small" type="submit">{text("Resolve versions", "查出套件版本")} <ArrowRight size={15}/></button>
      <span class="quiet mono">{text("Drop a lockfile here", "也可拖放 lockfile 至此")}</span>
    </div>
    <div classList={{ "notice studio-file-notice": Boolean(fileStatus()) }}>
      <p role="status">{fileStatus() === "oversize" ? text(
        "The browser preview accepts up to 400,000 bytes per file and up to 400,000 characters. Run the CLI from your project folder instead.",
        "瀏覽器預覽只支援 400,000 位元組及 400,000 個字元以內的檔案。請改在專案目錄執行 CLI。",
      ) : fileStatus() === "unreadable" ? text(
        "Couldn’t read this file. Try opening it again or paste its contents.", "無法讀取這個檔案。請重新開啟，或貼上檔案內容。",
      ) : ""}</p>
      <Show when={fileStatus() === "oversize"}><div class="studio-command">
        <code>{offlineCommand}</code>
        <CopyButton value={offlineCommand} label={text("Copy offline CLI command", "複製離線 CLI 指令")} analytics={{ surface: "playground_cli", command_kind: "cli" }}/>
      </div></Show>
    </div>
  </form>;
}
