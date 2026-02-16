import {createContext} from 'react';

export type Props = {
	readonly enableMouseTracking: () => void;
	readonly disableMouseTracking: () => void;
};

const MouseContext = createContext<Props>({
	enableMouseTracking() {},
	disableMouseTracking() {},
});

MouseContext.displayName = 'InternalMouseContext';

export default MouseContext;