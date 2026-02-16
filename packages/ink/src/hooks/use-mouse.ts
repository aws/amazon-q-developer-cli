import {useEffect, useContext, useRef, useCallback} from 'react';
import {type DOMElement} from '../dom.js';
import {type MouseEvent} from '../parse-mouse.js';
import MouseContext from '../components/MouseContext.js';
import StdinContext from '../components/StdinContext.js';
import measureElement from '../measure-element.js';

type UseMouseOptions = {
	isActive?: boolean;
	onClick?: (event: MouseEvent) => void;
	onRightClick?: (event: MouseEvent) => void;
	onScrollUp?: (event: MouseEvent) => void;
	onScrollDown?: (event: MouseEvent) => void;
};

const useMouse = (options: UseMouseOptions = {}) => {
	const {isActive = true, onClick, onRightClick, onScrollUp, onScrollDown} = options;
	const {enableMouseTracking, disableMouseTracking} = useContext(MouseContext);
	const {internal_eventEmitter} = useContext(StdinContext);
	const ref = useRef<DOMElement>(null);

	useEffect(() => {
		if (!isActive) return;
		enableMouseTracking();
		return () => disableMouseTracking();
	}, [isActive, enableMouseTracking, disableMouseTracking]);

	const handleMouse = useCallback(
		(event: MouseEvent) => {
			if (event.button === 'scrollUp') { onScrollUp?.(event); return; }
			if (event.button === 'scrollDown') { onScrollDown?.(event); return; }
			if (event.type !== 'press') return;

			if (ref.current) {
				const {width, height} = measureElement(ref.current);
				const left = ref.current.yogaNode?.getComputedLeft() ?? 0;
				const top = ref.current.yogaNode?.getComputedTop() ?? 0;
				const {col, row} = event;
				if (col < left || col >= left + width || row < top || row >= top + height) {
					return;
				}
			}

			if (event.button === 'left') onClick?.(event);
			if (event.button === 'right') onRightClick?.(event);
		},
		[onClick, onRightClick, onScrollUp, onScrollDown],
	);

	useEffect(() => {
		if (!isActive) return;
		internal_eventEmitter?.on('mouse', handleMouse);
		return () => {
			internal_eventEmitter?.removeListener('mouse', handleMouse);
		};
	}, [isActive, internal_eventEmitter, handleMouse]);

	return {ref};
};

export default useMouse;