import React, { useEffect, useState } from "react";
import {
  applyNodeChanges,
  Handle,
  Position,
  ReactFlow,
  type Node,
  type NodeChange,
  type NodeProps,
  type ReactFlowProps,
} from "@xyflow/react";
import BoltOutlinedIcon from "@mui/icons-material/BoltOutlined";
import BuildOutlinedIcon from "@mui/icons-material/BuildOutlined";
import CallSplitOutlinedIcon from "@mui/icons-material/CallSplitOutlined";
import AccountTreeOutlinedIcon from "@mui/icons-material/AccountTreeOutlined";
import FunctionsOutlinedIcon from "@mui/icons-material/FunctionsOutlined";
import { STYLES, type WorkflowNode } from "../../src/model";

export type GraphNode = Node<{ node: WorkflowNode; result?: string }>;
export const statuses: Record<string, string> = {
  RUNNING: "运行中",
  SUCCESS: "成功",
  FAILED: "失败",
  CANCELLED: "已取消",
  pending: "等待",
  running: "运行中",
  success: "成功",
  failed: "失败",
  skipped: "跳过",
};

/** Renders a compact node with standard graph handles and execution state. */
export function WorkflowCard({ data, selected }: NodeProps<GraphNode>) {
  const style = STYLES[data.node.type];
  const result = data.result;
  return (
    <div
      className={
        "graph-node" +
        (selected ? " selected" : "") +
        (result ? " is-" + result : "")
      }
      style={{ borderColor: style.color }}
    >
      {data.node.type !== "trigger" && (
        <Handle type="target" position={Position.Left} />
      )}
      <div className="node-kind" style={{ color: style.color }}>
        {style.label}
        {result && <span className="node-status">{statuses[result]}</span>}
      </div>
      <strong>{data.node.name}</strong>
      <div className="node-description">
        {data.node.description || "双击配置节点"}
      </div>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
export const nodeTypes = {
  workflow: React.memo(
    WorkflowCard,
    (previous, next) =>
      previous.data === next.data && previous.selected === next.selected,
  ),
};
export const fitOptions = { maxZoom: 1, padding: 0.2 };
export const flowOptions = { hideAttribution: true };

/** Keeps pointer-frequency node updates inside the canvas instead of the application shell. */
export function WorkflowCanvas({
  nodes: incomingNodes,
  ...props
}: ReactFlowProps<GraphNode> & { nodes: GraphNode[] }) {
  const [nodes, setNodes] = useState(incomingNodes);

  // React Flow emits internal changes while it measures and mounts nodes.  Keep
  // the synchronization out of render: calling setState from render can make a
  // transient empty prop win over the real graph and blank the whole canvas.
  useEffect(() => {
    setNodes(incomingNodes);
  }, [incomingNodes]);

  /** Applies positions, selection and measured dimensions only within this canvas. */
  const changeNodes = React.useCallback((changes: NodeChange<GraphNode>[]) => {
    setNodes((current) => {
      const safeChanges = changes.filter(
        (change) => change.type !== "remove",
      );
      return safeChanges.length === 0
        ? current
        : applyNodeChanges(safeChanges, current);
    });
  }, []);
  return <ReactFlow {...props} nodes={nodes} onNodesChange={changeNodes} />;
}
export const nodeIcons: Record<WorkflowNode["type"], React.ReactNode> = {
  trigger: <BoltOutlinedIcon />,
  execute: <BuildOutlinedIcon />,
  condition: <CallSplitOutlinedIcon />,
  logic: <AccountTreeOutlinedIcon />,
  extract: <FunctionsOutlinedIcon />,
};


