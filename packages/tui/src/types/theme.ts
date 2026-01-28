export interface Theme {
  name: string;
  colors: {
    primary: string;
    secondary: string;
    success: string;
    warning: string;
    error: string;
    info: string;
    text: string;
    textDim: string;
    border: string;
    background: string;
  };
}

export const defaultTheme: Theme = {
  name: 'default',
  colors: {
    primary: 'cyan',
    secondary: 'blue',
    success: 'green',
    warning: 'yellow',
    error: 'red',
    info: 'blue',
    text: 'white',
    textDim: 'gray',
    border: 'gray',
    background: 'black',
  },
};

export const noColorTheme: Theme = {
  name: 'no-color',
  colors: {
    primary: '',
    secondary: '',
    success: '',
    warning: '',
    error: '',
    info: '',
    text: '',
    textDim: '',
    border: '',
    background: '',
  },
};
