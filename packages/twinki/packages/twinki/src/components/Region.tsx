import React from 'react';
import { NODE_TYPES } from '../text/constants.js';

export interface RegionProps {
	id: string;
	children?: React.ReactNode;
}

/**
 * Region creates a render boundary in the component tree.
 * When state changes inside a Region, only that region's subtree
 * is re-rendered — the rest of the tree uses cached lines.
 */
export const Region: React.FC<RegionProps> = ({ id, children }) => {
	return React.createElement(NODE_TYPES.TWINKI_REGION, { regionId: id }, children);
};
