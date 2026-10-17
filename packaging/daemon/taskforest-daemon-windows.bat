@echo off
REM TaskForest Background Daemon Windows Service Management Helper
REM
REM Usage:
REM   taskforest-daemon-windows.bat install    (Registers Windows Service via sc.exe)
REM   taskforest-daemon-windows.bat start      (Starts the Windows Service)
REM   taskforest-daemon-windows.bat stop       (Stops the Windows Service)
REM   taskforest-daemon-windows.bat remove     (Deletes the Windows Service)
REM   taskforest-daemon-windows.bat run-bg     (Runs in background without Service Manager)

set SERVICE_NAME=TaskForestDaemon
set BIN_PATH="%~dp0taskforest-daemon.exe"

if "%1"=="install" goto do_install
if "%1"=="start" goto do_start
if "%1"=="stop" goto do_stop
if "%1"=="remove" goto do_remove
if "%1"=="run-bg" goto do_run_bg
goto do_usage

:do_install
echo Registering Windows Service: %SERVICE_NAME%
sc create %SERVICE_NAME% binPath= "%BIN_PATH%" start= auto DisplayName= "TaskForest Background Telemetry Service"
goto end

:do_start
echo Starting Windows Service: %SERVICE_NAME%
sc start %SERVICE_NAME%
goto end

:do_stop
echo Stopping Windows Service: %SERVICE_NAME%
sc stop %SERVICE_NAME%
goto end

:do_remove
echo Removing Windows Service: %SERVICE_NAME%
sc delete %SERVICE_NAME%
goto end

:do_run_bg
echo Starting taskforest-daemon in background...
start /b "" %BIN_PATH%
goto end

:do_usage
echo Usage: %0 {install^|start^|stop^|remove^|run-bg}
exit /b 1

:end
exit /b 0
