"use client";

import { useEffect, useRef, useState } from "react";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { useTheme } from "next-themes";
import { HugeiconsIcon } from "@hugeicons/react";
import {
	Delete02Icon,
	PlayIcon,
	StopIcon,
} from "@hugeicons/core-free-icons";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Button } from "@/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { cn } from "@/utils/ui";
import { useEditorApiBridge } from "@/api/bridge-store";
import { SCRIPT_EXAMPLES } from "../examples";
import { runScript, type ScriptLogEntry, type ScriptRunHandle } from "../script-runner";
import { useScriptingStore } from "../scripting-store";

const LOG_STYLES: Record<ScriptLogEntry["level"], string> = {
	log: "text-foreground",
	info: "text-foreground",
	warn: "text-caution",
	error: "text-destructive",
	system: "text-muted-foreground italic",
};

export function ScriptingView() {
	const bridge = useEditorApiBridge();
	const { code, logs, isRunning, setCode, appendLog, clearLogs, setRunning } =
		useScriptingStore();
	const runRef = useRef<ScriptRunHandle | null>(null);
	const logEndRef = useRef<HTMLDivElement>(null);
	const [exampleId, setExampleId] = useState<string>("");

	useEffect(() => {
		logEndRef.current?.scrollIntoView({ block: "end" });
	}, [logs.length]);

	useEffect(
		() => () => {
			runRef.current?.stop();
		},
		[],
	);

	const handleRun = () => {
		if (!bridge || isRunning) return;
		setRunning({ isRunning: true });
		const handle = runScript({
			code,
			bridge,
			onLog: (entry) => appendLog({ entry }),
		});
		runRef.current = handle;
		void handle.done.then(() => {
			if (runRef.current === handle) {
				runRef.current = null;
				setRunning({ isRunning: false });
			}
		});
	};

	const handleStop = () => {
		runRef.current?.stop();
	};

	const handleExample = (id: string) => {
		setExampleId(id);
		const example = SCRIPT_EXAMPLES.find((item) => item.id === id);
		if (example) setCode({ code: example.code });
	};

	return (
		<PanelView
			title="Scripting"
			contentClassName="flex h-full flex-col gap-2 px-2 pb-2"
			scrollClassName="overflow-hidden pt-2"
			actions={
				<div className="flex items-center gap-1">
					<Select value={exampleId} onValueChange={handleExample}>
						<SelectTrigger className="h-7 w-44 text-xs">
							<SelectValue placeholder="Examples" />
						</SelectTrigger>
						<SelectContent>
							{SCRIPT_EXAMPLES.map((example) => (
								<SelectItem key={example.id} value={example.id}>
									{example.title}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					{isRunning ? (
						<Button
							size="sm"
							variant="outline"
							className="h-7 gap-1 text-xs"
							onClick={handleStop}
						>
							<HugeiconsIcon icon={StopIcon} className="size-3.5" />
							Stop
						</Button>
					) : (
						<Button
							size="sm"
							className="h-7 gap-1 text-xs"
							onClick={handleRun}
							disabled={!bridge}
						>
							<HugeiconsIcon icon={PlayIcon} className="size-3.5" />
							Run
						</Button>
					)}
				</div>
			}
		>
			<CodeEditor
				value={code}
				onChange={(next) => setCode({ code: next })}
				onRun={handleRun}
				className="min-h-0 flex-[3]"
			/>
			<div className="flex min-h-0 flex-[2] flex-col rounded-md border">
				<div className="flex h-8 shrink-0 items-center justify-between border-b px-2">
					<span className="text-muted-foreground text-xs">Console</span>
					<Button
						variant="ghost"
						size="icon"
						className="size-6"
						onClick={clearLogs}
						aria-label="Clear console"
					>
						<HugeiconsIcon icon={Delete02Icon} className="size-3.5" />
					</Button>
				</div>
				<div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto p-2 font-mono text-xs leading-relaxed">
					{logs.length === 0 && (
						<p className="text-muted-foreground">
							Output from console.log and the script result appears here. Press
							Ctrl/Cmd+Enter to run.
						</p>
					)}
					{logs.map((entry) => (
						<div
							key={entry.id}
							className={cn("whitespace-pre-wrap break-words", LOG_STYLES[entry.level])}
						>
							{entry.text}
						</div>
					))}
					<div ref={logEndRef} />
				</div>
			</div>
		</PanelView>
	);
}

function CodeEditor({
	value,
	onChange,
	onRun,
	className,
}: {
	value: string;
	onChange: (value: string) => void;
	onRun: () => void;
	className?: string;
}) {
	const hostRef = useRef<HTMLDivElement>(null);
	const viewRef = useRef<EditorView | null>(null);
	const themeCompartment = useRef(new Compartment());
	const onChangeRef = useRef(onChange);
	const onRunRef = useRef(onRun);
	const { resolvedTheme } = useTheme();
	onChangeRef.current = onChange;
	onRunRef.current = onRun;

	useEffect(() => {
		const host = hostRef.current;
		if (!host) return;
		const view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					basicSetup,
					javascript(),
					keymap.of([
						{
							key: "Mod-Enter",
							run: () => {
								onRunRef.current();
								return true;
							},
						},
						indentWithTab,
					]),
					themeCompartment.current.of(resolvedTheme === "dark" ? oneDark : []),
					EditorView.updateListener.of((update) => {
						if (update.docChanged) {
							onChangeRef.current(update.state.doc.toString());
						}
					}),
					EditorView.theme({
						"&": { height: "100%", fontSize: "12px" },
						".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" },
					}),
				],
			}),
		});
		viewRef.current = view;
		return () => {
			view.destroy();
			viewRef.current = null;
		};
		// The editor owns its document after mount; external changes are pushed below.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	useEffect(() => {
		const view = viewRef.current;
		if (!view) return;
		const current = view.state.doc.toString();
		if (current !== value) {
			view.dispatch({
				changes: { from: 0, to: current.length, insert: value },
			});
		}
	}, [value]);

	useEffect(() => {
		viewRef.current?.dispatch({
			effects: themeCompartment.current.reconfigure(
				resolvedTheme === "dark" ? oneDark : [],
			),
		});
	}, [resolvedTheme]);

	return (
		<div
			ref={hostRef}
			className={cn("overflow-hidden rounded-md border [&_.cm-editor]:h-full", className)}
		/>
	);
}
