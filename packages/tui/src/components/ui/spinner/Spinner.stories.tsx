import { Spinner } from './Spinner.js';

type Meta<T> = {
  title: string;
  component: T;
  parameters?: Record<string, unknown>;
};

type StoryObj<T> = {
  args?: Record<string, unknown>;
};

const meta: Meta<typeof Spinner> = {
  title: 'UI/Spinner',
  component: Spinner,
  parameters: {
    docs: {
      description: {
        component: 'Animated braille dot spinner for loading/thinking states.',
      },
    },
  },
};

export default meta;
type Story = StoryObj<typeof Spinner>;

export const Default: Story = {
  args: {},
};
