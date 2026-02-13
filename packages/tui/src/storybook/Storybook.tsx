import React, { useState } from 'react';
import { Box, Text } from 'ink';
import { useTheme } from '../hooks/useThemeContext.js';
import { stories } from './stories.js';
import { useKeypress } from '../hooks/useKeypress.js';

type AppState = 'componentList' | 'componentView';

export function Storybook() {
  const [state, setState] = useState<AppState>('componentList');
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(
    new Set()
  );
  const [selectedVariant, setSelectedVariant] = useState(0);
  const { getColor } = useTheme();

  const primaryColor = getColor('primary');
  const textColor = getColor('secondary');
  const accentColor = getColor('accent');
  const successColor = getColor('success');

  // Group stories by category
  // Group stories by category (e.g., "UI/Radio/RadioButton" -> category is "UI/Radio")
  const groupedStories = stories.reduce(
    (acc, story, index) => {
      const category = story.category || 'Uncategorized';
      if (!acc[category]) {
        acc[category] = [];
      }
      acc[category].push({ story, index });
      return acc;
    },
    {} as Record<string, Array<{ story: (typeof stories)[0]; index: number }>>
  );

  const categories = Object.keys(groupedStories).sort();

  // Build flat list of navigable items (section headers, category folders, and components)
  const buildNavigableItems = () => {
    const items: Array<
      | { type: 'sectionHeader'; name: string; componentCount: number }
      | { type: 'category'; name: string; componentCount: number }
      | {
          type: 'component';
          categoryName: string;
          story: (typeof stories)[0];
          storyIndex: number;
        }
    > = [];

    // Group categories by their top-level section (e.g., "UI", "Components")
    const sectionGroups: Record<string, string[]> = {};
    categories.forEach((category) => {
      const section = category.split('/')[0]!;
      if (!sectionGroups[section]) {
        sectionGroups[section] = [];
      }
      sectionGroups[section]!.push(category);
    });

    // Process each section
    Object.keys(sectionGroups)
      .sort()
      .forEach((section) => {
        const sectionCategories = sectionGroups[section]!;

        // Add section header (always non-expandable)
        const totalComponents = sectionCategories.reduce(
          (sum, cat) => sum + groupedStories[cat]!.length,
          0
        );
        items.push({
          type: 'sectionHeader',
          name: section,
          componentCount: totalComponents,
        });

        // Group by subfolder within this section
        // For "UI/Radio/RadioButton" and "UI/Radio/RadioGroup", group by "Radio"
        // For "UI/Chip/Chip", it's just "Chip"
        const subfolderMap: Record<
          string,
          Array<{ story: (typeof stories)[0]; index: number; category: string }>
        > = {};

        sectionCategories.forEach((category) => {
          const parts = category.split('/');
          let subfolderKey: string;

          if (parts.length >= 2) {
            // Extract subfolder name (e.g., "UI/Radio/RadioButton" -> "Radio")
            subfolderKey = parts[1]!;
          } else {
            // Shouldn't happen with our current structure, but handle it
            subfolderKey = category;
          }

          if (!subfolderMap[subfolderKey]) {
            subfolderMap[subfolderKey] = [];
          }

          // Add all components from this category to the subfolder
          groupedStories[category]!.forEach(({ story, index }) => {
            subfolderMap[subfolderKey]!.push({ story, index, category });
          });
        });

        // Process each subfolder
        Object.keys(subfolderMap)
          .sort()
          .forEach((subfolderName) => {
            const components = subfolderMap[subfolderName]!;
            const isSingleComponent = components.length === 1;
            const subfolderFullName = `${section}/${subfolderName}`;

            if (isSingleComponent) {
              // Single component - show flat
              const component = components[0]!;
              items.push({
                type: 'component',
                categoryName: component.category,
                story: component.story,
                storyIndex: component.index,
              });
            } else {
              // Multiple components - show as expandable category
              items.push({
                type: 'category',
                name: subfolderFullName,
                componentCount: components.length,
              });

              // If expanded, show components
              if (expandedCategories.has(subfolderFullName)) {
                components.forEach(({ story, index, category }) => {
                  items.push({
                    type: 'component',
                    categoryName: category,
                    story,
                    storyIndex: index,
                  });
                });
              }
            }
          });
      });

    return items;
  };

  const navigableItems = buildNavigableItems();

  // Find the first navigable item (skip section headers)
  const firstNavigableIndex = navigableItems.findIndex(
    (item) => item.type === 'component' || item.type === 'category'
  );
  const [selectedIndex, setSelectedIndex] = useState(
    Math.max(0, firstNavigableIndex)
  );

  // Calculate max name length for alignment (only for components and categories)
  const maxNameLength = navigableItems.reduce((max, item) => {
    if (item.type === 'component') {
      return Math.max(max, item.story.name.length);
    } else if (item.type === 'category') {
      const displayName = item.name.split('/').pop() || item.name;
      return Math.max(max, displayName.length);
    }
    return max;
  }, 0);

  useKeypress((input, key) => {
    if (state === 'componentList') {
      if (key.upArrow) {
        // Move to previous navigable item (skip section headers)
        setSelectedIndex((prev) => {
          let newIndex = prev - 1;
          while (
            newIndex >= 0 &&
            navigableItems[newIndex]?.type === 'sectionHeader'
          ) {
            newIndex--;
          }
          return Math.max(0, newIndex);
        });
      } else if (key.downArrow) {
        // Move to next navigable item (skip section headers)
        setSelectedIndex((prev) => {
          let newIndex = prev + 1;
          while (
            newIndex < navigableItems.length &&
            navigableItems[newIndex]?.type === 'sectionHeader'
          ) {
            newIndex++;
          }
          return Math.min(navigableItems.length - 1, newIndex);
        });
      } else if (key.return || key.rightArrow) {
        const selectedItem = navigableItems[selectedIndex];
        if (!selectedItem) return;
        if (selectedItem.type === 'category') {
          // Toggle category expansion
          setExpandedCategories((prev) => {
            const next = new Set(prev);
            if (next.has(selectedItem.name)) {
              next.delete(selectedItem.name);
            } else {
              next.add(selectedItem.name);
            }
            return next;
          });
        } else if (selectedItem.type === 'component') {
          // Navigate to component view
          setState('componentView');
          setSelectedVariant(0);
        }
      } else if (key.leftArrow) {
        const selectedItem = navigableItems[selectedIndex];
        if (!selectedItem) return;
        if (
          selectedItem.type === 'category' &&
          expandedCategories.has(selectedItem.name)
        ) {
          // Close expanded category
          setExpandedCategories((prev) => {
            const next = new Set(prev);
            next.delete(selectedItem.name);
            return next;
          });
        } else if (selectedItem.type === 'component') {
          // If on a component in an expanded category, close the category
          const categoryName = selectedItem.categoryName;
          if (expandedCategories.has(categoryName)) {
            const categoryIndex = navigableItems.findIndex(
              (item) => item.type === 'category' && item.name === categoryName
            );

            setExpandedCategories((prev) => {
              const next = new Set(prev);
              next.delete(categoryName);
              return next;
            });

            if (categoryIndex !== -1) {
              setSelectedIndex(categoryIndex);
            }
          }
        }
      } else if (key.escape) {
        process.exit(0);
      }
    } else if (state === 'componentView') {
      const selectedItem = navigableItems[selectedIndex];
      if (!selectedItem || selectedItem.type !== 'component') return;

      const currentStory = stories[selectedItem.storyIndex];
      if (!currentStory) return;
      const currentVariant = currentStory.variants[selectedVariant];
      if (!currentVariant) return;
      const capturesKeyboard = currentVariant.parameters?.capturesKeyboard;

      if (capturesKeyboard) {
        if (key.escape) {
          setState('componentList');
        }
        return;
      }

      if (key.leftArrow) {
        if (selectedVariant === 0) {
          setState('componentList');
        } else {
          setSelectedVariant((prev) => prev - 1);
        }
      } else if (key.rightArrow) {
        const maxVariants = currentStory.variants.length - 1;
        setSelectedVariant((prev) => Math.min(maxVariants, prev + 1));
      } else if (key.escape) {
        setState('componentList');
      }
    }
  });

  const renderComponentList = () => (
    <Box flexDirection="column" height="100%">
      <Box flexDirection="row" alignItems="center" gap={0} marginBottom={1}>
        <Text>{primaryColor('📚 TUI Storybook')}</Text>
        <Text>
          <Text color="blackBright"> | </Text>
          <Text>{accentColor('↑↓')}</Text>
          <Text color="blackBright"> navigate, </Text>
          <Text>{accentColor('→')}</Text>
          <Text color="blackBright"> select/expand, </Text>
          <Text>{accentColor('Esc')}</Text>
          <Text color="blackBright"> exit</Text>
        </Text>
      </Box>
      <Box flexDirection="column">
        {navigableItems.map((item, index) => {
          const isSelected = index === selectedIndex;

          if (item.type === 'sectionHeader') {
            // Section header - non-interactive label
            return (
              <Text key={`section-${item.name}`} bold>
                {'\n'}
                {'  '}
                {item.name}
                {'\n'}
              </Text>
            );
          } else if (item.type === 'category') {
            // Category folder - expandable/collapsible
            const isExpanded = expandedCategories.has(item.name);
            // Extract just the folder name (e.g., "UI/Radio" -> "Radio")
            const displayName = item.name.split('/').pop() || item.name;
            const paddedName = displayName.padEnd(maxNameLength + 2);
            return (
              <Text
                key={`cat-${item.name}`}
                color={isSelected ? 'green' : undefined}
              >
                {isSelected ? '▶ ' : '  '}
                {paddedName}
                {isExpanded ? '▴' : '▾'}
              </Text>
            );
          } else {
            // Component item
            // Check if this component is in an expanded category (needs extra indent)
            // item.categoryName is like "UI/Radio/RadioButton", category name is like "UI/Radio"
            const isInExpandedCategory = navigableItems.some(
              (i) =>
                i.type === 'category' &&
                item.categoryName.startsWith(i.name + '/')
            );
            const indent = isInExpandedCategory ? '  ' : '';
            return (
              <Text
                key={`comp-${item.story.name}`}
                color={isSelected ? 'green' : undefined}
              >
                {isSelected ? '▶ ' : '  '}
                {indent}
                {item.story.name}
              </Text>
            );
          }
        })}
      </Box>
    </Box>
  );

  const renderComponentView = () => {
    const selectedItem = navigableItems[selectedIndex];
    if (!selectedItem || selectedItem.type !== 'component') return null;

    const currentComponent = stories[selectedItem.storyIndex];
    if (!currentComponent) return null;
    const currentVariant = currentComponent.variants[selectedVariant];
    if (!currentVariant) return null;
    const Component = currentComponent.component;

    return (
      <Box flexDirection="column" height="100%">
        <Box flexDirection="row" alignItems="center" gap={1} marginBottom={1}>
          <Text>{primaryColor('📚 TUI Storybook')}</Text>
          <Text color="blackBright"> - </Text>
          <Text>{accentColor(currentComponent.name)}</Text>
          <Text>
            <Text color="grey"> | </Text>
            {currentVariant.parameters?.capturesKeyboard ? (
              <>
                <Text>{successColor('🎹 Keyboard Captured')}</Text>
                <Text color="gray"> - </Text>
                <Text>{accentColor('Esc')}</Text>
                <Text color="gray"> to exit</Text>
              </>
            ) : (
              <>
                <Text>{accentColor('← →')}</Text>
                <Text color="gray"> Switch variants, </Text>
                <Text>{accentColor('←')}</Text>
                <Text color="gray"> on first variant goes back</Text>
              </>
            )}
          </Text>
        </Box>

        <Box marginBottom={1}>
          <Text>{textColor('Variants: ')}</Text>
          {currentComponent.variants.map((variant, index) => (
            <Text
              key={variant.name}
              color={index === selectedVariant ? 'green' : 'gray'}
            >
              {index > 0 ? ' | ' : ''}
              {index === selectedVariant ? '[' : ''}
              {variant.name}
              {index === selectedVariant ? ']' : ''}
            </Text>
          ))}
        </Box>

        {/* Props Display */}
        <Box marginBottom={1} flexDirection="row">
          <Text>{textColor('Props: ')}</Text>
          {Object.keys(currentVariant.props).length > 0 ? (
            <Text>
              <Text color="gray">{'{ '}</Text>
              {Object.entries(currentVariant.props).map(
                ([key, value], index, array) => {
                  let displayValue: string;

                  if (typeof value === 'string') {
                    displayValue = `"${value}"`;
                  } else if (typeof value === 'boolean') {
                    displayValue = String(value);
                  } else if (typeof value === 'number') {
                    displayValue = String(value);
                  } else if (typeof value === 'function') {
                    displayValue = '[Function]';
                  } else if (value === undefined) {
                    displayValue = 'undefined';
                  } else if (value === null) {
                    displayValue = 'null';
                  } else if (React.isValidElement(value)) {
                    // Handle React elements
                    const elementType =
                      typeof value.type === 'string'
                        ? value.type
                        : value.type?.name || 'Component';
                    const childrenProp = (value.props as any)?.children;
                    if (typeof childrenProp === 'string') {
                      displayValue = `<${elementType}>${childrenProp}</${elementType}>`;
                    } else {
                      displayValue = `<${elementType} />`;
                    }
                  } else if (typeof value === 'object' && value !== null) {
                    // Handle plain objects
                    try {
                      displayValue = JSON.stringify(value, null, 0);
                    } catch {
                      displayValue = '[Object]';
                    }
                  } else {
                    displayValue = String(value);
                  }

                  const isOptional =
                    currentVariant.parameters?.optionalProps?.includes(key);

                  return (
                    <Text key={key}>
                      <Text color="cyan">{key}</Text>
                      {isOptional && <Text color="gray">?</Text>}
                      <Text color="gray">: </Text>
                      <Text color="yellow">{displayValue}</Text>
                      {index < array.length - 1 && <Text color="gray">, </Text>}
                    </Text>
                  );
                }
              )}
              <Text color="gray">{' }'}</Text>
            </Text>
          ) : currentVariant.component ? (
            <Text color="gray">Custom component (no props)</Text>
          ) : (
            <Text color="gray">{'{}'}</Text>
          )}
        </Box>

        <Box flexGrow={1}>
          {currentVariant.component
            ? React.createElement(currentVariant.component, {
                key: `${selectedItem.storyIndex}-${selectedVariant}`,
                ...currentVariant.props,
              })
            : React.createElement(Component as any, {
                key: `${selectedItem.storyIndex}-${selectedVariant}`,
                ...currentVariant.props,
              })}
        </Box>
      </Box>
    );
  };

  switch (state) {
    case 'componentList':
      return renderComponentList();
    case 'componentView':
      return renderComponentView();
    default:
      return renderComponentList();
  }
}
