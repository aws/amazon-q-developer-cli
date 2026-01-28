import { Box } from 'ink';
import React from 'react';
import { Write } from '../chat/tools/Write.js';
import { StatusInfo } from '../ui/status/StatusInfo.js';

export interface WriteToolMessageProps {
  content: string;
  /** Line offset for status bar coloring (accounts for label + margin) */
  lineOffset?: number;
}

export const WriteToolMessage: React.FC<WriteToolMessageProps> = ({ content }) => {
  try {
    const parsed = JSON.parse(content);
    const { 
      command, 
      path, 
      content: fileContent,
      oldStr,
      newStr,
      insertLine,
    } = parsed;

    let title: string;
    let oldText = '';
    let newText = '';

    switch (command) {
      case 'create':
        title = 'Creating';
        newText = fileContent || '';
        break;
      case 'strReplace':
        title = 'Replacing in';
        oldText = oldStr || '';
        newText = newStr || '';
        break;
      case 'insert':
        title = insertLine !== undefined 
          ? `Inserting at line ${insertLine} in`
          : 'Appending to';
        newText = fileContent || '';
        break;
      default:
        title = 'Writing';
        newText = fileContent || '';
    }

    if (!newText && !oldText) {
      return null;
    }

    // Label (1 line) + marginTop={1} (1 line) = 2 lines before diff
    const diffLineOffset = 2;

    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={path} />
        <Box marginTop={1}>
          <Write oldText={oldText} newText={newText} filePath={path} lineOffset={diffLineOffset} />
        </Box>
      </Box>
    );
  } catch {
    return null;
  }
};
