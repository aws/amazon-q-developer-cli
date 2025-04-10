import { Component, ErrorInfo, ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
}

class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
  };

  public static getDerivedStateFromError(_: Error): State {
    // Update state so the next render will show the fallback UI.
    return { hasError: true };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Uncaught error:", error, errorInfo);

    // Reload the page in 5 seconds
    setTimeout(() => {
      window.location.reload();
    }, 5000);
  }

  public render() {
    if (this.state.hasError) {
      return <></>;
    }

    return this.props.children;
  }
}

export default ErrorBoundary;
