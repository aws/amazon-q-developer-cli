import renderNodeToOutput, {
	renderNodeToScreenReaderOutput,
} from './render-node-to-output.js';
import Output from './output.js';
import {type DOMElement} from './dom.js';

type Result = {
	output: string;
	outputHeight: number;
	staticOutput: string;
};

// Cache static output so we only re-render when new <Static> children are added.
// Without this, ink re-renders all static content every frame and returns it as
// staticOutput, which causes the caller to clear + rewrite it repeatedly,
// producing ghost/duplicate lines on screen.
let lastStaticChildCount = 0;
let cachedStaticOutput = '';

const renderer = (node: DOMElement, isScreenReaderEnabled: boolean): Result => {
	if (node.yogaNode) {
		if (isScreenReaderEnabled) {
			const output = renderNodeToScreenReaderOutput(node, {
				skipStaticElements: true,
			});

			const outputHeight = output === '' ? 0 : output.split('\n').length;

			let staticOutput = '';

			if (node.staticNode) {
				staticOutput = renderNodeToScreenReaderOutput(node.staticNode, {
					skipStaticElements: false,
				});
			}

			return {
				output,
				outputHeight,
				staticOutput: staticOutput ? `${staticOutput}\n` : '',
			};
		}

		const output = new Output({
			width: node.yogaNode.getComputedWidth(),
			height: node.yogaNode.getComputedHeight(),
		});

		renderNodeToOutput(node, output, {
			skipStaticElements: true,
		});

		let staticOutput = '';

		if (node.staticNode?.yogaNode) {
			const childCount = node.staticNode.childNodes
				? node.staticNode.childNodes.length
				: 0;

			// Only re-render static content when new children have been added
			if (childCount !== lastStaticChildCount) {
				lastStaticChildCount = childCount;

				const staticBuf = new Output({
					width: node.staticNode.yogaNode.getComputedWidth(),
					height: node.staticNode.yogaNode.getComputedHeight(),
				});

				renderNodeToOutput(node.staticNode, staticBuf, {
					skipStaticElements: false,
				});

				cachedStaticOutput = `${staticBuf.get().output}\n`;
				staticOutput = cachedStaticOutput;
			}
		}

		const {output: generatedOutput, height: outputHeight} = output.get();

		return {
			output: generatedOutput,
			outputHeight,
			staticOutput,
		};
	}

	return {
		output: '',
		outputHeight: 0,
		staticOutput: '',
	};
};

export default renderer;
