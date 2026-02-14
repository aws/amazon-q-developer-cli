import { createContext, useContext } from 'react';

export const AnimationPausedContext = createContext(false);
export const useAnimationPaused = () => useContext(AnimationPausedContext);
