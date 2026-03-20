import { PieSpinner } from './PieSpinner.js';

type Meta<T> = {
  title: string;
  component: T;
  parameters?: Record<string, unknown>;
};

type StoryObj = {
  args?: Record<string, unknown>;
};

const meta: Meta<typeof PieSpinner> = {
  title: 'UI/Spinner/PieSpinner',
  component: PieSpinner,
  parameters: {
    docs: {
      description: {
        component:
          'Animated pie chart spinner for tool in-progress states. Cycles through ◔ → ◑ → ◕ → ● to show clockwise fill.',
      },
    },
  },
};

export default meta;
type Story = StoryObj;

export const Default: Story = {
  args: {},
};
