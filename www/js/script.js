window.onload = function() {
	function drawMe() {
		navigator.geolocation.watchPosition(function(position) {
			var latitude = position.coords.latitude;
			var longitude = position.coords.longitude;
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			var mapOptions = {
				center: gLatLng,
				zoom: 18,
				mapTypeId: google.maps.MapTypeId.ROADMAP,
				draggable: false,
				zoomControl: false,
				scrollwheel: false,
				disableDoubleClickZoom: true,
				disableDefaultUI: true,
			};
			var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
			var yourLocation = {
				strokeColor: "#FF0000",
				strokeOpacity: 0.5,
				strokeWeight: 1,
				fillColor: "#FF0000",
				fillOpacity: 1,
				map: map,
				center: gLatLng,
				radius: 4,
			};
			var drawCircle = new google.maps.Circle(yourLocation);
		}, function(error) {
			alert('Error occurred. Error code: ' + error.code);
		});
	}
	drawMe();
	$(window).on("resize", function(){
		drawMe();
	});
};