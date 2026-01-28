import React, { useState, useEffect } from 'react';
import { Box } from 'ink';
import { Text } from '../ui/text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';

interface WordmarkProps {
  animate?: boolean;
}

export default function Wordmark({ animate = false }: WordmarkProps) {
  const { getColor } = useTheme();
  const brandColor = getColor('brand');

  // Animation state
  const [visibleLetters, setVisibleLetters] = useState(animate ? 0 : 4);

  useEffect(() => {
    if (!animate) return;

    const timer = setInterval(() => {
      setVisibleLetters((prev) => (prev < 4 ? prev + 1 : 4));
    }, 300);

    // Reset animation every 5 seconds
    // const resetTimer = setInterval(() => {
    //     setVisibleLetters(0);
    // }, 5000);

    return () => {
      clearInterval(timer);
      // clearInterval(resetTimer);
    };
  }, [animate]);

  const letterK = ` ⢀⣴⣶⣶⣦⡀⠀⠀⠀⠀⢀⣴⣶⣦⣄⡀
⢰⣿⠋⠁⠈⠙⣿⡆⠀⢀⣾⡿⠁  ⠈⢻⡆
⢸⣿⠀⠀⠀⠀⣿⣇⣴⡿⠋⠀⠀  ⢀⣼⠇
⢸⣿⠀⠀⠀⠀⣿⡿⠋⠀⠀  ⢀⣾⡿⠁
⢸⣿⠀⠀⠀⠀⠙⠁⠀⠀ ⢀⣼⡟⠁
⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀ ⠹⣷⡀
⢸⣿⠀⠀⠀⠀⠀⣠⡀⠀⠀ ⠹⣷⡄
⢸⣿⠀⠀⠀⠀⣾⡟⣷⡀⠀⠀ ⠘⣿⣆
⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀  ⠀⠈⢻⡇
⠸⣿⣄⡀⢀⣠⣿⠇⠀⠙⣷⡀  ⢀⣼⠇
 ⠈⠻⠿⠿⠟⠁⠀⠀⠀⠈⠻⠿⠿⠟⠁`;

  const letterI = ` ⢀⣴⣶⣶⣦⡀
⢰⣿⠋⠁⠈⠙⣿⡆
⢸⣿⠀⠀⠀⠀⣿⡇
⢸⣿⠀⠀⠀⠀⣿⡇
⢸⣿⠀⠀⠀⠀⣿⡇
⢸⣿⠀⠀⠀⠀⣿⡇⠀
⢸⣿⠀⠀⠀⠀⣿⡇⠀
⢸⣿⠀⠀⠀⠀⣿⡇
⢸⣿⠀⠀⠀⠀⣿⡇
⠸⣿⣄⡀⢀⣠⣿⠇
 ⠈⠻⠿⠿⠟⠁`;

  const letterR = ` ⢀⣴⣶⣶⣶⣶⣶⣶⣶⣶⣶⣦⣄⡀
⢰⣿⠋⠁        ⠈⠙⠻⣦
⢸⣿⠀⠀⠀⢠⣤⣤⣤⣤⣄    ⣿⡆
⢸⣿⠀⠀⠀⢸⣿⠉⠉⠉⣿⡇   ⣿⡇ 
⢸⣿⠀⠀⠀⢸⣿⣶⣶⡶⠋⠀   ⣿⠇
⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀   ⣠⣼⠟
⢸⣿⠀⠀⠀⠀⣤⣄  ⠀⠀⠹⣿⡅
⢸⣿⠀⠀⠀⠀⣿⡟⣷⡀⠀⠀ ⠘⣿⣆
⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀  ⠀⠈⢻⡇
⠸⣿⣄⡀⢀⣠⣿⠇⠀⠙⣷⡀  ⢀⣼⠇
 ⠈⠻⠿⠿⠟⠁⠀⠀⠀⠈⠻⠿⠿⠟⠁`;

  const letterO = `    ⢀⣠⣴⣶⣶⣶⣶⣶⣦⣄⡀
   ⣴⡿⠟⠋⠁   ⠈⠙⠻⢿⣦
  ⣼⡟⠀⠀⠀ ⣀⣀⣀    ⢻⣧
 ⣼⡟⠀⠀ ⣰⡿⠟⠛⠻⢿⣆⠀⠀ ⢻⣧
⢰⣿⠀⠀⠀⢰⣿⠀⠀⠀  ⣿⡆⠀⠀ ⣿⡆
⢸⣿⠀⠀ ⢸⣿⠀⠀⠀⠀ ⣿⡇⠀⠀ ⣿⡇
⠸⣿⠀⠀ ⠸⣿⠀⠀⠀⠀ ⣿⠇⠀  ⣿⠇
 ⢻⣧⠀⠀ ⠹⣷⣦⣤⣤⣾⠏⠀⠀⠀⣼⡟
  ⢻⣧⠀⠀⠀ ⠉⠉⠉    ⣼⡟
   ⠻⣷⣦⣄⡀   ⢀⣠⣴⣾⠟
   ⠀⠀⠈⠙⠻⠿⠿⠿⠿⠟⠋⠁`;

  //   const letterK = ` _____     ___
  // //‾‾‾\\\\   //‾‾\\\\
  // ││   ││ //    //
  // ││   │//    //
  // ││    ‾    \\\\
  // ││    ˍ_    \\\\
  // ││   //\\\\    \\\\
  // ││   ││ \\\\    \\\\
  // \\\\___//  \\\\___//
  //  ‾‾‾‾‾    ‾‾‾‾`;

  //   const letterI = ` _____
  // //‾‾‾\\\\
  // ││   ││
  // ││   ││
  // ││   ││
  // ││   ││
  // ││   ││
  // ││   ││
  // \\\\___//
  //  ‾‾‾‾‾`;

  //   const letterR = ` ____________
  // //‾‾‾‾‾‾‾‾‾‾‾\\\\
  // ││    ____    \\\\
  // ││   ││‾‾//   ││
  // ││    ‾‾‾     //
  // ││    __     //
  // ││   ││\\\\    \\\\
  // ││   ││ \\\\    \\\\
  // \\\\___//  \\\\___//
  //  ‾‾‾‾‾    ‾‾‾‾`;

  //   const letterO = `     _______
  //    // ‾‾‾‾‾ \\\\
  //  //   _____   \\\\
  // ││   //‾‾‾\\\\   ││
  // ││   ││   ││   ││
  // ││   ││   ││   ││
  // ││   \\\\___//   ││
  //  \\\\   ‾‾‾‾‾   //
  //    \\\\ˍ_____ˍ//
  //      ‾‾‾‾‾‾‾`;

  const letters = [letterK, letterI, letterR, letterO];

  return (
    <Box width="100%" justifyContent="center">
      <Box flexDirection="row">
        {letters.map((letter, index) => (
          <React.Fragment key={index}>
            <Text>{index < visibleLetters ? brandColor(letter) : ''}</Text>
            {index === 0 && index < visibleLetters && <Text> </Text>}
          </React.Fragment>
        ))}
      </Box>
    </Box>
  );
}
