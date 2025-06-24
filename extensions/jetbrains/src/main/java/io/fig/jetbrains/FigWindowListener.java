package io.fig.jetbrains;

import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.openapi.wm.ex.ToolWindowManagerListener;
import org.jetbrains.annotations.NotNull;

import java.util.List;

public class FigWindowListener implements ToolWindowManagerListener {

    private static boolean INITIALIZED = false;

    @Override
    public void toolWindowsRegistered(@NotNull List<String> ids, @NotNull ToolWindowManager toolWindowManager) {
        if (INITIALIZED)
            return;

        INITIALIZED = true;

        FigService service = ApplicationManager.getApplication().getService(FigService.class);
        service.initContentManager(toolWindowManager);
    }
}
