import React, { useState, useCallback } from 'react';

export function useConversationContent() {
  const [isStreaming, setIsStreaming] = useState(false);
  const [children, setChildren] = useState<React.ReactNode[]>([]);

  const startStreaming = useCallback(() => {
    setIsStreaming(true);
    // Don't clear existing children - content stacks and stays
  }, []);

  const addChild = useCallback((child: React.ReactNode) => {
    setChildren((prev) => [...prev, child]);
  }, []);

  const addChildWithBarControl = useCallback((child: React.ReactNode) => {
    setChildren((prev) => {
      const lineIndex = prev.length; // Next available line
      const childWithProps = React.cloneElement(
        child as React.ReactElement,
        { lineIndex } as any
      );
      return [...prev, childWithProps];
    });
  }, []);

  const updateChild = useCallback(
    (index: number, newChild: React.ReactNode) => {
      setChildren((prev) =>
        prev.map((child, i) => (i === index ? newChild : child))
      );
    },
    []
  );

  const removeChild = useCallback((index: number) => {
    setChildren((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const stopStreaming = useCallback(() => {
    setIsStreaming(false);
    // Content stays in place - never cleared
  }, []);

  // Removed clearStreaming - content should never be cleared once added

  return {
    isStreaming,
    children,
    startStreaming,
    addChild,
    addChildWithBarControl,
    updateChild,
    removeChild,
    stopStreaming,
  };
}
