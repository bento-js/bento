// Addressable identity for workflow subagents: the readable registry id derived
// from an agent's label, and the IRC block appended to every workflow prompt.
import type { WorkflowAgentStatus } from "./display.js";

// OMP's agent registry keys the coordinator session as "Main" (MAIN_AGENT_ID in
// `registry/agent-registry`). A workflow agent that allocated that id would
// overwrite the coordinator's ref, so a label deriving to it is pushed aside.
const COORDINATOR_ID = "Main";

// Long enough for three or four words; ids also name transcript files, so an
// unbounded label would produce an unreadable artifact name.
const MAX_ID_LENGTH = 40;

export interface WorkflowPeer {
    agentId: string;
    label: string;
    status: WorkflowAgentStatus;
    /** Set only for peers belonging to another live run. */
    runName?: string;
}

export interface WorkflowIrcContext {
    selfId: string;
    coordinatorId: string;
    runId: string;
    runName: string;
    peers: WorkflowPeer[];
    foreignPeers: WorkflowPeer[];
}

/**
 * Derive a readable registry id from an agent label: `runtime-core` ->
 * `RuntimeCore`. The result is a base name; uniqueness across a session is the
 * caller's job (OMP's AgentOutputManager allocates `Base`, `Base-2`, …).
 */
export function agentIdFromLabel(label: string): string {
    let words = label.match(/[A-Za-z0-9]+/g) ?? [];
    let id = words
        .map(word => word[0].toUpperCase() + word.slice(1))
        .join("")
        .slice(0, MAX_ID_LENGTH);
    // An id opening with a digit reads as a task index in `agent://` and in
    // artifact names, so such a label keeps the runtime's generic prefix.
    if (!/^[A-Za-z]/.test(id)) id = `Workflow${id}`;
    return id === COORDINATOR_ID ? `${id}Agent` : id;
}

/** The IRC block appended to a workflow agent's prompt. */
export function renderIrcBlock(context: WorkflowIrcContext): string {
    let lines = [
        "## IRC",
        `You are \`${context.selfId}\` in workflow \`${context.runName}\` (run ${context.runId}).`,
        'Message any agent below with the `write` tool: `path: "agent://<id>"`, plain text content.',
        `Coordinator: \`${context.coordinatorId}\` — started this workflow and keeps working while you run.`,
        peerSection("Peers in this run:", context.peers, "none yet; more agents may still start"),
    ];
    if (context.foreignPeers.length > 0) {
        lines.push(peerSection("Peers in other live workflow runs:", context.foreignPeers, ""));
    }
    lines.push(
        "Message a peer when you change something they depend on or need an answer only they have; message the coordinator only for a blocking question or a decision you cannot make. Otherwise keep working.",
        // A workflow agent that parks on `wait` stalls its whole run: nobody is
        // obliged to answer it, and the run cannot finish until it yields.
        "Never wait for a message to arrive and never idle for an assignment: finish the work above and yield your result.",
    );
    return lines.join("\n");
}

/** Append the IRC block to a prompt, keeping one blank line between them. */
export function appendIrcBlock(prompt: string, context: WorkflowIrcContext): string {
    return `${prompt.trimEnd()}\n\n${renderIrcBlock(context)}`;
}

function peerSection(title: string, peers: WorkflowPeer[], empty: string): string {
    if (peers.length === 0) return `${title} ${empty}`;
    return [title, ...peers.map(peerRow)].join("\n");
}

function peerRow(peer: WorkflowPeer): string {
    let run = peer.runName ? ` in \`${peer.runName}\`` : "";
    return `- \`${peer.agentId}\` — ${peer.label} [${peer.status}]${run}`;
}
