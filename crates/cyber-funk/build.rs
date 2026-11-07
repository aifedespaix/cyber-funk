//! Icône de l'exécutable Windows (Explorateur, barre des tâches). Sans effet ailleurs.

fn main() {
    println!("cargo:rerun-if-changed=icon.rc");
    println!("cargo:rerun-if-changed=../../assets/images/icons/app.ico");
    #[cfg(windows)]
    {
        // Le chemin de l'.ico dans icon.rc est relatif au dossier du crate (répertoire de travail du build script).
        // Un échec n'arrête pas le build (l'exécutable n'aurait que l'icône par défaut) mais est signalé.
        if let Err(error) = embed_resource::compile("icon.rc", embed_resource::NONE).manifest_optional() {
            println!("cargo:warning=icône de l'exécutable non embarquée : {error}");
        }
    }
}
