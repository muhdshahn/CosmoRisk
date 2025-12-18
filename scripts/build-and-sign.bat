@echo off
:: CosmoRisk Build & Sign Script
:: Version: 2.0.1
:: This script builds the app and signs it automatically

echo ================================================
echo   CosmoRisk Build ^& Sign - v2.0.1
echo   Developer: Mehmet Gumus
echo ================================================
echo.

cd /d "C:\Users\mehme\Desktop\CosmoRisk"

echo [1/3] Building application...
call npm run tauri build

if %errorlevel% neq 0 (
    echo.
    echo [WARNING] Build had errors, but exe might still be created.
    echo Continuing with signing...
)

echo.
echo [2/3] Signing executable...
signtool sign /sha1 FBA0F34D3439787A5B1767018899E1B510A14E34 /fd sha256 /tr http://timestamp.digicert.com /td sha256 "C:\Users\mehme\Desktop\CosmoRisk\src-tauri\target\release\cosmorisk.exe"

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Signing failed!
    pause
    exit /b 1
)

echo.
echo [3/3] Verifying signature...
signtool verify /pa "C:\Users\mehme\Desktop\CosmoRisk\src-tauri\target\release\cosmorisk.exe"

echo.
echo ================================================
echo   SUCCESS!
echo ================================================
echo.
echo Signed executable location:
echo C:\Users\mehme\Desktop\CosmoRisk\src-tauri\target\release\cosmorisk.exe
echo.
echo Remember to:
echo 1. Test the exe
echo 2. Upload to VirusTotal
echo 3. Upload to GitHub Releases
echo.
pause

