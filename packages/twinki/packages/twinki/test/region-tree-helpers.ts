/**
 * Builds the tree shape that pegged the CPU in the field: a heavy conversation
 * region plus a spinner region whose text changes on a 100ms interval.
 */
import { createNode, createTextNode } from '../src/reconciler/node-factory.js';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';
import { NODE_TYPES } from '../src/text/constants.js';
import type { RootContainer, TwinkiNode } from '../src/reconciler/types.js';

export function buildLargeTree(messageCount: number): {
	root: RootContainer;
	spinnerText: TwinkiNode;
	spinnerRegion: TwinkiNode;
} {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

	const root: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	// Heavy conversation region (simulates chat messages)
	const chatRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'chat' });
	for (let i = 0; i < messageCount; i++) {
		const box = createNode(NODE_TYPES.TWINKI_BOX as any, { flexDirection: 'column' });
		const textNode = createNode(NODE_TYPES.TWINKI_TEXT as any, { wrap: 'wrap' });
		const rawText = createTextNode(`Message ${i}: ${'Lorem ipsum dolor sit amet. '.repeat(3)}`);
		textNode.children.push(rawText);
		rawText.parent = textNode;
		box.children.push(textNode);
		textNode.parent = box;
		if (textNode.yogaNode) box.yogaNode!.insertChild(textNode.yogaNode, box.yogaNode!.getChildCount());
		chatRegion.children.push(box);
		box.parent = chatRegion;
		if (box.yogaNode) chatRegion.yogaNode!.insertChild(box.yogaNode, chatRegion.yogaNode!.getChildCount());
	}
	root.children.push(chatRegion);
	chatRegion.parent = null;
	if (chatRegion.yogaNode) root.yogaNode.insertChild(chatRegion.yogaNode, root.yogaNode.getChildCount());

	// Spinner region (simulates the status bar spinner)
	const spinnerRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'spinner' });
	const spinnerText = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
	const spinnerRaw = createTextNode('⠋');
	spinnerText.children.push(spinnerRaw);
	spinnerRaw.parent = spinnerText;
	spinnerRegion.children.push(spinnerText);
	spinnerText.parent = spinnerRegion;
	if (spinnerText.yogaNode) spinnerRegion.yogaNode!.insertChild(spinnerText.yogaNode, spinnerRegion.yogaNode!.getChildCount());
	root.children.push(spinnerRegion);
	spinnerRegion.parent = null;
	if (spinnerRegion.yogaNode) root.yogaNode.insertChild(spinnerRegion.yogaNode, root.yogaNode.getChildCount());

	return { root, spinnerText, spinnerRegion };
}

export const SPINNER_FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
